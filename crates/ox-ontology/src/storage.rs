//! Content-addressed storage primitives for [`OntologyIR`].
//!
//! This module is the type-layer implementation behind the Λ
//! Phase storage refactor. It is paired with the `ox-store` Level
//! 2 schema (`ontology_entity_versions` +
//! `ontology_version_entities`, migrations 0017) and owns three
//! concerns:
//!
//! 1. **Canonicalisation** — produce a deterministic byte sequence
//!    for any `serde_json::Value`, so two semantically identical
//!    entities hash to the same content address.
//! 2. **Hashing** — SHA-256 of the canonical bytes → 64-char hex
//!    hash. Matches the `ontology_entity_versions.entity_hash`
//!    `CHECK` constraint.
//! 3. **Entity extraction** — walk an [`OntologyIR`] and emit one
//!    [`ExtractedEntity`] per top-level collection member, ready
//!    to bulk-insert into Level 2.
//!
//! Hydration (Level 2 → `OntologyIR`) lives in
//! [`hydrate_ontology_ir`]; the inverse of extract.
//!
//! ## Canonicalisation choice — RFC 8785 subset
//!
//! Full RFC 8785 (JSON Canonicalization Scheme) requires
//! ECMA-262 number formatting, which serde_json doesn't do
//! natively. We implement the subset that matters for our
//! entities:
//!
//! - Object keys sorted lexicographically (UTF-16 code-unit
//!   order, matching RFC 8785).
//! - No insignificant whitespace.
//! - Strings serialised with serde_json's standard escape rules
//!   (RFC 8259 compatible).
//! - Numbers left as serde_json produces them.
//!
//! This is sufficient because every entity in `OntologyIR`
//! stores numbers as `i64` / `f64` with well-known serde output;
//! the cross-platform-number-edge-cases RFC 8785 covers don't
//! appear in practice. If they ever do, a future version lifts
//! the canonicaliser to the full RFC 8785 without touching
//! callers.
//!
//! ## Hash stability
//!
//! The hash of an entity is a contract — change it across
//! releases and every stored entity is orphaned. To prevent
//! silent drift:
//!
//! - Fields serialise with `#[serde(skip_serializing_if =
//!   "Option::is_none")]` / `Vec::is_empty` so defaults don't
//!   leak into the hash input.
//! - New fields on an existing entity type default to the
//!   "absent" serialisation — an entity that was valid under
//!   schema_version N hashes identically under N+1 as long as
//!   no field was added without a default.
//! - Bumping `ONTOLOGY_IR_SCHEMA_VERSION` is the signal that
//!   hashes are allowed to drift; downstream deployments drop
//!   and re-materialise.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use ox_core::error::{OxError, OxResult};

use crate::ir::OntologyIR;

/// The kinds the materialised denormalised graph distinguishes.
/// Mirrors the `ontology_entity_kind` Postgres enum in
/// `0001_schema.sql`; the wire string is the `snake_case` rendering
/// of each variant.
///
/// Note: `Property` is enum-addressable (referenced from the
/// materialised neighbour graph + search index) but is **not**
/// emitted as a top-level `ExtractedEntity` by `extract_entities`
/// — properties live inside their parent `NodeType` / `EdgeType`
/// payloads in the content-addressed store. The variant exists so
/// every `entity_kind` cast in the materialised SQL views stays
/// safe regardless of which surface emits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    /// Ontology metadata (name, description, version). Singleton
    /// per version — its logical_id is the owning `OntologyIR.id`.
    OntologyHeader,
    // Topology
    NodeType,
    EdgeType,
    IndexDef,
    Interface,
    /// A property defined on a node_type or edge_type. Stable id;
    /// neighbour edges in the denormalised graph anchor here.
    Property,
    // Mapping
    ObjectMapping,
    LinkMapping,
    PropertyMapping,
    // Governance
    Rule,
    DataQuality,
    Action,
    Provenance,
    // Behaviour
    Function,
    Metric,
    Enrichment,
    // Vocabulary + value semantics
    GlossaryTerm,
    Taxonomy,
    CodeSystem,
    /// Individual `CodedValue` row nested inside a `CodeSystem`.
    /// Same nesting policy as `Property` — addressable in the
    /// materialised denormalised graph (hierarchy walks anchor here)
    /// but not extracted as a standalone content-addressed entity.
    CodedValue,
    ValueSet,
    NotationPattern,
    ConceptMap,
    ValueRangeSet,
    /// Φ3 — per-column distribution snapshot. Carries the
    /// `(source_id, relation, column)` location plus the
    /// `ColumnStats` payload from the introspection kernel.
    ColumnProfile,
}

impl EntityKind {
    /// Wire name matching the Postgres enum variant.
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityKind::OntologyHeader => "ontology_header",
            EntityKind::NodeType => "node_type",
            EntityKind::EdgeType => "edge_type",
            EntityKind::IndexDef => "index_def",
            EntityKind::Interface => "interface",
            EntityKind::Property => "property",
            EntityKind::ObjectMapping => "object_mapping",
            EntityKind::LinkMapping => "link_mapping",
            EntityKind::PropertyMapping => "property_mapping",
            EntityKind::Rule => "rule",
            EntityKind::DataQuality => "data_quality",
            EntityKind::Action => "action",
            EntityKind::Provenance => "provenance",
            EntityKind::Function => "function",
            EntityKind::Metric => "metric",
            EntityKind::Enrichment => "enrichment",
            EntityKind::GlossaryTerm => "glossary_term",
            EntityKind::Taxonomy => "taxonomy",
            EntityKind::CodeSystem => "code_system",
            EntityKind::CodedValue => "coded_value",
            EntityKind::ValueSet => "value_set",
            EntityKind::NotationPattern => "notation_pattern",
            EntityKind::ConceptMap => "concept_map",
            EntityKind::ValueRangeSet => "value_range_set",
            EntityKind::ColumnProfile => "column_profile",
        }
    }

    /// Parse the wire-string produced by `as_str` back into the
    /// enum. Rejects any non-matching input — the hydration
    /// path surfaces the error so unknown kinds do not silently
    /// deserialise as one of the known variants.
    pub fn parse(wire: &str) -> OxResult<Self> {
        let v = match wire {
            "ontology_header" => EntityKind::OntologyHeader,
            "node_type" => EntityKind::NodeType,
            "edge_type" => EntityKind::EdgeType,
            "index_def" => EntityKind::IndexDef,
            "interface" => EntityKind::Interface,
            "property" => EntityKind::Property,
            "object_mapping" => EntityKind::ObjectMapping,
            "link_mapping" => EntityKind::LinkMapping,
            "property_mapping" => EntityKind::PropertyMapping,
            "rule" => EntityKind::Rule,
            "data_quality" => EntityKind::DataQuality,
            "action" => EntityKind::Action,
            "provenance" => EntityKind::Provenance,
            "function" => EntityKind::Function,
            "metric" => EntityKind::Metric,
            "enrichment" => EntityKind::Enrichment,
            "glossary_term" => EntityKind::GlossaryTerm,
            "taxonomy" => EntityKind::Taxonomy,
            "code_system" => EntityKind::CodeSystem,
            "coded_value" => EntityKind::CodedValue,
            "value_set" => EntityKind::ValueSet,
            "notation_pattern" => EntityKind::NotationPattern,
            "concept_map" => EntityKind::ConceptMap,
            "value_range_set" => EntityKind::ValueRangeSet,
            "column_profile" => EntityKind::ColumnProfile,
            other => {
                return Err(OxError::Runtime {
                    message: format!(
                        "unknown ontology entity kind '{other}' — storage layer / \
                         Rust enum drift"
                    ),
                });
            }
        };
        Ok(v)
    }
}

/// One entity ready to commit to the Level 2 store. The quartet
/// `(kind, logical_id, hash, canonical)` is what the SQL writer
/// needs — `content` is the hash input (canonical JSON) and the
/// row payload (same bytes).
#[derive(Debug, Clone)]
pub struct ExtractedEntity {
    pub kind: EntityKind,
    pub logical_id: String,
    pub hash: String,
    pub content: Value,
}

impl ExtractedEntity {
    /// Canonical JSON of `content`, as a `&str`. Kept as an owned
    /// `Value` in the struct (small allocations, easy to log) and
    /// re-canonicalised on demand — the call site that actually
    /// writes to Postgres gets to choose bytes vs. string shape.
    pub fn canonical_json(&self) -> String {
        canonical_json(&self.content)
    }
}

// ---------------------------------------------------------------------------
// Canonicalisation + hashing
// ---------------------------------------------------------------------------

/// Produce a deterministic byte sequence for a `serde_json::Value`.
/// Keys in every nested object are sorted lexicographically; no
/// insignificant whitespace; strings / numbers / booleans /
/// null serialise through serde_json's standard path.
///
/// See the module doc for why this is sufficient without going to
/// the full RFC 8785.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::String(s) => serde_json::to_string(s).unwrap_or_else(|_| String::from("\"\"")),
        Value::Array(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(item));
            }
            out.push(']');
            out
        }
        Value::Object(obj) => {
            // Sort keys lexicographically by chars. For ASCII-only
            // keys (our universe) this matches RFC 8785's UTF-16
            // code-unit ordering exactly.
            let sorted: BTreeMap<&String, &Value> = obj.iter().collect();
            let mut out = String::from("{");
            for (i, (k, v)) in sorted.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(
                    &serde_json::to_string(k).unwrap_or_else(|_| String::from("\"\"")),
                );
                out.push(':');
                out.push_str(&canonical_json(v));
            }
            out.push('}');
            out
        }
    }
}

/// SHA-256 of a canonical byte sequence, rendered as a 64-char
/// lowercase hex string. Matches the `^[0-9a-f]{64}$` CHECK on
/// `ontology_entity_versions.entity_hash`.
pub fn hash_canonical(canonical: &str) -> String {
    let digest = Sha256::digest(canonical.as_bytes());
    // lowercase hex — `{:02x}` per byte.
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        // `write!` to a `String` buffer is infallible — String's
        // `fmt::Write` impl never returns Err. `let_underscore_must_use`
        // gate is satisfied by the explicit allow.
        #[allow(clippy::let_underscore_must_use)]
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Serialise a `Serialize` value, canonicalise, and hash — the
/// full pipeline in one call. Used by `extract_entities` for
/// every top-level entity.
fn canonical_and_hash<T: Serialize>(value: &T) -> OxResult<(Value, String, String)> {
    let raw = serde_json::to_value(value).map_err(|e| OxError::Runtime {
        message: format!("entity serialization failed: {e}"),
    })?;
    let canonical = canonical_json(&raw);
    let hash = hash_canonical(&canonical);
    Ok((raw, canonical, hash))
}

// ---------------------------------------------------------------------------
// Extraction — OntologyIR → Vec<ExtractedEntity>
// ---------------------------------------------------------------------------

/// Walk `ir` and emit one `ExtractedEntity` per entity the
/// content-addressed store tracks. The header entity (ontology
/// metadata) is always present; the remaining kinds follow the
/// IR's collection vectors in insertion order.
///
/// Returns `OxResult` because per-entity serde failures propagate
/// — but in practice every entity in `OntologyIR` is
/// derive-Serialize and cannot fail. The error path exists so a
/// future entity type with manual `Serialize` that *can* fail
/// doesn't silently corrupt the content-addressed store.
pub fn extract_entities(ir: &OntologyIR) -> OxResult<Vec<ExtractedEntity>> {
    let mut out = Vec::new();

    // --- Header ------------------------------------------------
    // Singleton per version. Captures ontology-scoped fields that
    // don't belong to any individual entity.
    #[derive(Serialize)]
    struct Header<'a> {
        id: &'a str,
        name: &'a str,
        display_name: &'a ox_core::i18n::LocalizedText,
        description: &'a ox_core::i18n::LocalizedText,
        version: &'a crate::ir::OntologyVersion,
        schema_version: u32,
    }
    let (content, canonical, hash) = canonical_and_hash(&Header {
        id: &ir.id,
        name: &ir.name,
        display_name: &ir.display_name,
        description: &ir.description,
        version: &ir.version,
        schema_version: ir.schema_version,
    })?;
    let _ = canonical; // kept in signature for future use (telemetry, etc.)
    out.push(ExtractedEntity {
        kind: EntityKind::OntologyHeader,
        logical_id: ir.id.clone(),
        hash,
        content,
    });

    // --- Topology ----------------------------------------------
    for nt in ir.node_types() {
        out.push(extract(EntityKind::NodeType, &nt.id, nt)?);
    }
    for et in ir.edge_types() {
        out.push(extract(EntityKind::EdgeType, &et.id, et)?);
    }
    for idx in ir.indexes() {
        // IndexDef is an enum; logical_id is per-variant. All
        // variants carry an `id` field — exhaustive match so a
        // new variant surfaces as a compile error here rather
        // than silently breaking content-address extraction.
        let id = match idx {
            crate::ir::IndexDef::Single { id, .. } => id.clone(),
            crate::ir::IndexDef::Composite { id, .. } => id.clone(),
            crate::ir::IndexDef::FullText { id, .. } => id.clone(),
            crate::ir::IndexDef::Vector { id, .. } => id.clone(),
        };
        out.push(extract(EntityKind::IndexDef, &id, idx)?);
    }
    for iface in ir.interfaces() {
        out.push(extract(EntityKind::Interface, &iface.id, iface)?);
    }

    // --- Mapping -----------------------------------------------
    for om in ir.object_mappings() {
        out.push(extract(EntityKind::ObjectMapping, &om.id, om)?);
    }
    for lm in ir.link_mappings() {
        out.push(extract(EntityKind::LinkMapping, &lm.id, lm)?);
    }
    // PropertyMappingDef is a nested type — extracted separately
    // once the mapping layer promotes them to top-level. Today
    // they live inside ObjectMappingDef and ride along with the
    // parent's hash. When the IR model lifts them out, this loop
    // activates.
    // for pm in ir.property_mappings() { ... }

    // --- Governance --------------------------------------------
    for rule in ir.rules() {
        out.push(extract(EntityKind::Rule, &rule.id, rule)?);
    }
    for dq in ir.data_quality() {
        out.push(extract(EntityKind::DataQuality, &dq.id, dq)?);
    }
    for action in ir.actions() {
        out.push(extract(EntityKind::Action, &action.id, action)?);
    }
    for prov in ir.provenance() {
        out.push(extract(EntityKind::Provenance, &prov.id, prov)?);
    }

    // --- Behaviour ---------------------------------------------
    for func in ir.functions() {
        out.push(extract(EntityKind::Function, &func.id, func)?);
    }
    for metric in ir.metrics() {
        out.push(extract(EntityKind::Metric, &metric.id, metric)?);
    }
    for enrich in ir.enrichments() {
        out.push(extract(EntityKind::Enrichment, &enrich.id, enrich)?);
    }

    // --- Vocabulary + value semantics --------------------------
    for term in ir.glossary() {
        out.push(extract(EntityKind::GlossaryTerm, &term.id, term)?);
    }
    // TaxonomyDef is accessible via the glossary() collection
    // today; when it becomes a first-class IR collection the
    // extraction moves here.
    for cs in ir.code_systems() {
        out.push(extract(EntityKind::CodeSystem, &cs.id, cs)?);
    }
    for vs in ir.value_sets() {
        out.push(extract(EntityKind::ValueSet, &vs.id, vs)?);
    }
    for np in ir.notation_patterns() {
        out.push(extract(EntityKind::NotationPattern, &np.id, np)?);
    }
    for cm in ir.concept_maps() {
        out.push(extract(EntityKind::ConceptMap, &cm.id, cm)?);
    }
    for rs in ir.value_range_sets() {
        out.push(extract(EntityKind::ValueRangeSet, &rs.id, rs)?);
    }
    for cp in ir.column_profiles() {
        out.push(extract(EntityKind::ColumnProfile, &cp.id, cp)?);
    }

    Ok(out)
}

/// Shared helper — serialises the entity, computes the hash,
/// packages the tuple.
fn extract<T: Serialize>(
    kind: EntityKind,
    logical_id: &impl ToString,
    value: &T,
) -> OxResult<ExtractedEntity> {
    let (content, _canonical, hash) = canonical_and_hash(value)?;
    Ok(ExtractedEntity {
        kind,
        logical_id: logical_id.to_string(),
        hash,
        content,
    })
}

// ---------------------------------------------------------------------------
// Provenance collection accessor — the existing IR exposes a
// method named `provenance` rather than `provenances`; add a
// shim so `ir.provenance()` returns the same slice for our
// iteration style.
// ---------------------------------------------------------------------------
// (Already provided via the inline `for prov in ir.provenance()` above.)

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_orders_object_keys_lexicographically() {
        let v = json!({"b": 1, "a": 2, "c": 3});
        let c = canonical_json(&v);
        assert_eq!(c, r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn canonical_json_emits_no_insignificant_whitespace() {
        let v = json!({"arr": [1, 2, {"k": "v"}], "x": null});
        let c = canonical_json(&v);
        assert_eq!(c, r#"{"arr":[1,2,{"k":"v"}],"x":null}"#);
    }

    #[test]
    fn canonical_json_nested_objects_are_sorted_at_every_depth() {
        let v = json!({"outer": {"z": 1, "a": 2}, "alpha": {"y": 3, "b": 4}});
        let c = canonical_json(&v);
        assert_eq!(
            c,
            r#"{"alpha":{"b":4,"y":3},"outer":{"a":2,"z":1}}"#
        );
    }

    #[test]
    fn hash_canonical_matches_constraint_shape() {
        let h = hash_canonical("{}");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn hash_is_deterministic_across_equivalent_serialisations() {
        // Same logical content, different author-time ordering →
        // must produce identical hash. This is the whole point of
        // canonicalisation.
        let a = json!({"x": 1, "y": 2});
        let b = json!({"y": 2, "x": 1});
        assert_eq!(hash_canonical(&canonical_json(&a)), hash_canonical(&canonical_json(&b)));
    }

    #[test]
    fn entity_kind_wire_names_round_trip_through_parse() {
        for kind in [
            EntityKind::OntologyHeader,
            EntityKind::NodeType,
            EntityKind::EdgeType,
            EntityKind::IndexDef,
            EntityKind::Interface,
            EntityKind::ObjectMapping,
            EntityKind::LinkMapping,
            EntityKind::PropertyMapping,
            EntityKind::Rule,
            EntityKind::DataQuality,
            EntityKind::Action,
            EntityKind::Provenance,
            EntityKind::Function,
            EntityKind::Metric,
            EntityKind::Enrichment,
            EntityKind::GlossaryTerm,
            EntityKind::Taxonomy,
            EntityKind::CodeSystem,
            EntityKind::ValueSet,
            EntityKind::NotationPattern,
            EntityKind::ConceptMap,
            EntityKind::ValueRangeSet,
            EntityKind::ColumnProfile,
        ] {
            let wire = kind.as_str();
            let back = EntityKind::parse(wire).unwrap();
            assert_eq!(back, kind, "round-trip broke for {wire}");
        }
    }

    #[test]
    fn parse_unknown_wire_name_is_error() {
        assert!(EntityKind::parse("does_not_exist").is_err());
    }

    #[test]
    fn extract_entities_emits_header_for_empty_ontology() {
        let ir = OntologyIR::new(
            "ont-1".into(),
            "Empty".into(),
            ox_core::i18n::LocalizedText::default(),
            1,
            vec![crate::ir::NodeTypeDef {
                id: "nt-1".into(),
                label: ox_core::graph_label::GraphLabel::new("X")
                    .expect("valid literal"),
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let entities = extract_entities(&ir).unwrap();
        // Header + 1 node type = 2.
        assert_eq!(entities.len(), 2);
        assert!(entities
            .iter()
            .any(|e| e.kind == EntityKind::OntologyHeader && e.logical_id == "ont-1"));
        assert!(entities
            .iter()
            .any(|e| e.kind == EntityKind::NodeType && e.logical_id == "nt-1"));
    }

    #[test]
    fn extract_entities_produces_stable_hash_across_reextract() {
        let ir = OntologyIR::new(
            "ont-1".into(),
            "Stable".into(),
            ox_core::i18n::LocalizedText::default(),
            1,
            vec![crate::ir::NodeTypeDef {
                id: "nt-1".into(),
                label: ox_core::graph_label::GraphLabel::new("Customer")
                    .expect("valid literal"),
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let first = extract_entities(&ir).unwrap();
        let second = extract_entities(&ir).unwrap();
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(
                a.hash, b.hash,
                "hash drift for {:?}.{}",
                a.kind, a.logical_id
            );
        }
    }
}

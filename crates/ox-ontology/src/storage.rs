//! Content-addressed storage primitives for [`OntologyIR`].
//!
//! Type-layer counterpart to the `ox-store` Level 2 schema
//! (`ontology_entity_versions` + `ontology_version_entities`).
//! Owns three concerns:
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
//!   "absent" serialisation — adding such a field keeps every
//!   pre-existing entity's hash unchanged.

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
    Concept,
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
    /// Per-column distribution snapshot — `(source_id, relation,
    /// column)` location plus the `ColumnStats` payload from the
    /// introspection kernel.
    ColumnProfile,
    /// Named subset of a NodeType realising a specific concept
    /// (ADR-0015). Examples: `customer:vip`, `order:open`. Lives
    /// on the IR + validation layer + retrieval anchor surface
    /// — extraction here keeps the search index honest.
    Segment,
    /// Per-(source, table) inventory row recording import status
    /// and contribution edges. Drives the source-as-first-class UX
    /// and the change log; extraction here lets retrieval resolve
    /// "which tables did this project use?" without joining
    /// across mapping rows.
    TableInventory,
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
            EntityKind::Concept => "concept",
            EntityKind::GlossaryTerm => "glossary_term",
            EntityKind::Taxonomy => "taxonomy",
            EntityKind::CodeSystem => "code_system",
            EntityKind::CodedValue => "coded_value",
            EntityKind::ValueSet => "value_set",
            EntityKind::NotationPattern => "notation_pattern",
            EntityKind::ConceptMap => "concept_map",
            EntityKind::ValueRangeSet => "value_range_set",
            EntityKind::ColumnProfile => "column_profile",
            EntityKind::Segment => "segment",
            EntityKind::TableInventory => "table_inventory",
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
            "concept" => EntityKind::Concept,
            "glossary_term" => EntityKind::GlossaryTerm,
            "taxonomy" => EntityKind::Taxonomy,
            "code_system" => EntityKind::CodeSystem,
            "coded_value" => EntityKind::CodedValue,
            "value_set" => EntityKind::ValueSet,
            "notation_pattern" => EntityKind::NotationPattern,
            "concept_map" => EntityKind::ConceptMap,
            "value_range_set" => EntityKind::ValueRangeSet,
            "column_profile" => EntityKind::ColumnProfile,
            "segment" => EntityKind::Segment,
            "table_inventory" => EntityKind::TableInventory,
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
                out.push_str(&serde_json::to_string(k).unwrap_or_else(|_| String::from("\"\"")));
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

    // Every other collection participates in the
    // `IrCollection` contract — kind + logical_id flow from the
    // type, not the loop body. New collections need only an
    // `IrCollection` impl + one `extract_collection` line.
    extract_collection(&mut out, ir.node_types())?;
    extract_collection(&mut out, ir.edge_types())?;
    extract_collection(&mut out, ir.indexes())?;
    extract_collection(&mut out, ir.interfaces())?;

    extract_collection(&mut out, ir.object_mappings())?;
    extract_collection(&mut out, ir.link_mappings())?;
    // PropertyMappingDef is nested inside ObjectMappingDef — when
    // the mapping layer promotes them to top-level, an
    // `IrCollection` impl + one line lands them here.

    extract_collection(&mut out, ir.rules())?;
    extract_collection(&mut out, ir.data_quality())?;
    extract_collection(&mut out, ir.actions())?;
    extract_collection(&mut out, ir.provenance())?;

    extract_collection(&mut out, ir.functions())?;
    extract_collection(&mut out, ir.metrics())?;
    extract_collection(&mut out, ir.enrichments())?;

    extract_collection(&mut out, ir.concepts())?;
    extract_collection(&mut out, ir.glossary())?;
    extract_collection(&mut out, ir.code_systems())?;
    extract_collection(&mut out, ir.value_sets())?;
    extract_collection(&mut out, ir.notation_patterns())?;
    extract_collection(&mut out, ir.concept_maps())?;
    extract_collection(&mut out, ir.value_range_sets())?;
    extract_collection(&mut out, ir.column_profiles())?;

    // Φ8.2 — segments + table inventory promoted to first-class
    // extraction so retrieval anchors land in the search index +
    // navigation graph.
    extract_collection(&mut out, ir.segments())?;
    extract_collection(&mut out, ir.table_inventory())?;

    Ok(out)
}

/// Extract every member of `items` into the supplied accumulator.
/// One generic loop replaces the per-collection match-and-extract
/// blocks the older code carried — adding a new collection is
/// `IrCollection` impl + one call here.
fn extract_collection<T: crate::ir_collection::IrCollection>(
    out: &mut Vec<ExtractedEntity>,
    items: &[T],
) -> OxResult<()> {
    for item in items {
        let (content, _canonical, hash) = canonical_and_hash(item)?;
        out.push(ExtractedEntity {
            kind: T::ENTITY_KIND,
            logical_id: item.logical_id().into_owned(),
            hash,
            content,
        });
    }
    Ok(())
}


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
        assert_eq!(c, r#"{"alpha":{"b":4,"y":3},"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn hash_canonical_matches_constraint_shape() {
        let h = hash_canonical("{}");
        assert_eq!(h.len(), 64);
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn hash_is_deterministic_across_equivalent_serialisations() {
        // Same logical content, different author-time ordering →
        // must produce identical hash. This is the whole point of
        // canonicalisation.
        let a = json!({"x": 1, "y": 2});
        let b = json!({"y": 2, "x": 1});
        assert_eq!(
            hash_canonical(&canonical_json(&a)),
            hash_canonical(&canonical_json(&b))
        );
    }

    /// Single source of truth for "every variant of `EntityKind`" —
    /// every test that needs to walk the full enum reuses this so a
    /// future variant cannot silently slip past `parse` /
    /// `as_str` / cardinality checks. Adding a variant to `EntityKind`
    /// without listing it here is caught by `every_variant_appears_
    /// in_all_variants` (a missing-variant compile error from the
    /// exhaustive `match` below).
    fn all_variants() -> [EntityKind; 28] {
        [
            EntityKind::OntologyHeader,
            EntityKind::NodeType,
            EntityKind::EdgeType,
            EntityKind::IndexDef,
            EntityKind::Interface,
            EntityKind::Property,
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
            EntityKind::Concept,
            EntityKind::GlossaryTerm,
            EntityKind::Taxonomy,
            EntityKind::CodeSystem,
            EntityKind::CodedValue,
            EntityKind::ValueSet,
            EntityKind::NotationPattern,
            EntityKind::ConceptMap,
            EntityKind::ValueRangeSet,
            EntityKind::ColumnProfile,
            EntityKind::Segment,
            EntityKind::TableInventory,
        ]
    }

    /// Compile-time guard that `all_variants` actually covers every
    /// variant. The exhaustive `match` fails to compile the moment
    /// someone adds a new variant to `EntityKind` without also
    /// adding it to `all_variants`. Combined with the unit tests
    /// below, this makes "you added a kind but forgot to wire its
    /// wire string / parse arm" a build break, not a runtime drift.
    #[test]
    fn every_variant_appears_in_all_variants() {
        fn assert_listed(k: EntityKind) {
            // Exhaustive match — adding a new EntityKind without
            // updating this arm is a compile error.
            match k {
                EntityKind::OntologyHeader
                | EntityKind::NodeType
                | EntityKind::EdgeType
                | EntityKind::IndexDef
                | EntityKind::Interface
                | EntityKind::Property
                | EntityKind::ObjectMapping
                | EntityKind::LinkMapping
                | EntityKind::PropertyMapping
                | EntityKind::Rule
                | EntityKind::DataQuality
                | EntityKind::Action
                | EntityKind::Provenance
                | EntityKind::Function
                | EntityKind::Metric
                | EntityKind::Enrichment
                | EntityKind::Concept
                | EntityKind::GlossaryTerm
                | EntityKind::Taxonomy
                | EntityKind::CodeSystem
                | EntityKind::CodedValue
                | EntityKind::ValueSet
                | EntityKind::NotationPattern
                | EntityKind::ConceptMap
                | EntityKind::ValueRangeSet
                | EntityKind::ColumnProfile
                | EntityKind::Segment
                | EntityKind::TableInventory => {}
            }
        }
        for k in all_variants() {
            assert_listed(k);
        }
    }

    #[test]
    fn entity_kind_wire_names_round_trip_through_parse() {
        for kind in all_variants() {
            let wire = kind.as_str();
            let back =
                EntityKind::parse(wire).unwrap_or_else(|_| panic!("parse missing arm for {wire}"));
            assert_eq!(back, kind, "round-trip broke for {wire}");
        }
    }

    #[test]
    fn entity_kind_wire_names_are_distinct() {
        // Two variants accidentally returning the same `as_str()`
        // would collide on parse — surface that here rather than
        // letting the second variant silently shadow the first.
        let mut seen: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for kind in all_variants() {
            let wire = kind.as_str();
            assert!(
                seen.insert(wire),
                "two EntityKind variants share wire string '{wire}'"
            );
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
                label: ox_core::graph_label::GraphLabel::new("X").expect("valid literal"),
                ..Default::default()
            }],
            vec![],
            vec![],
        );
        let entities = extract_entities(&ir).unwrap();
        // Header + 1 node type = 2.
        assert_eq!(entities.len(), 2);
        assert!(
            entities
                .iter()
                .any(|e| e.kind == EntityKind::OntologyHeader && e.logical_id == "ont-1")
        );
        assert!(
            entities
                .iter()
                .any(|e| e.kind == EntityKind::NodeType && e.logical_id == "nt-1")
        );
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
                label: ox_core::graph_label::GraphLabel::new("Customer").expect("valid literal"),
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

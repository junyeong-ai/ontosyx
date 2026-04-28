//! `SourceMappingArtifact` — declarative bridge between a source
//! schema snapshot and the OntologyIR derived from it.
//!
//! The design pipeline used to inline the source-to-IR translation
//! inside the LLM prompt: the operator clicked "design", a single
//! call returned an `LlmDesignOutput` that the API turned into an
//! `OntologyIR` snapshot, and the per-column / per-FK mapping
//! decisions evaporated. Reproducing the same shape required
//! re-running the LLM against the same source — there was no
//! durable record of "for this source schema (at this hash), here
//! is which column → property and which FK → edge".
//!
//! `SourceMappingArtifact` makes that bridge a first-class,
//! versioned, queryable record. Each artifact captures:
//!
//! - `schema_snapshot_hash` — content hash of the
//!   [`SourceSchema`](ox_core::source_schema::SourceSchema) the
//!   mapping was authored against. Same source + same schema = same
//!   hash; rerunning analyze with no schema change produces a
//!   no-op. The artifact is content-addressed via this + the body
//!   hash so duplicate writes collapse.
//! - `property_mappings` — column → ontology property bindings.
//! - `edge_mappings` — FK / bridge / computed / federated edge
//!   bindings, mirroring the [`crate::mapping::LinkMappingKind`]
//!   variants.
//! - `open_questions` — ambiguities the LLM flagged for operator
//!   review (e.g. "column `customer_id` could be a FK to either
//!   `customers` or `accounts`").
//! - `provenance` — prompt id + version + model id + author so a
//!   later viewer can answer "who / what produced this artifact?"
//!
//! Stardog SMS2 / TopBraid R2RML inhabit the same niche; the goal
//! is identical (declarative mapping that survives across LLM
//! reruns) and the shape is intentionally compatible — re-running
//! `analyze` against an unchanged schema replays the previous
//! artifact instead of inventing a new IR.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::define_id_newtype;
use ox_core::i18n::LocalizedText;
use ox_core::source_schema::SourceSchema;

use crate::ir::{EdgeTypeId, OntologyIR};
use crate::mapping::{ColumnRef, LinkMappingKind, PropertyMappingDef, SourceId};

define_id_newtype!(
    /// Stable identifier for a [`SourceMappingArtifact`]. Minted on
    /// creation and never reassigned — same id, same body.
    SourceMappingArtifactId
);

/// Snapshot of one source's column / FK mapping decisions, keyed
/// by the schema hash they were authored against.
///
/// The artifact is content-addressed: the store layer derives a
/// SHA-256 hash from the canonical-JSON body and dedupes on insert.
/// Two callers writing the same artifact (same schema hash + same
/// body) collapse to one row, so the lifecycle path can re-emit
/// idempotently without the caller tracking what's already
/// persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct SourceMappingArtifact {
    pub id: SourceMappingArtifactId,
    /// The data source the schema lives in.
    pub source_id: SourceId,
    /// SHA-256 of the canonical-JSON serialisation of the
    /// [`SourceSchema`](ox_core::source_schema::SourceSchema) at
    /// authoring time. Same input = same hash; a column add,
    /// rename, or type change produces a new hash and therefore a
    /// new artifact.
    pub schema_snapshot_hash: String,
    /// Column → ontology property bindings. Reuses the canonical
    /// [`PropertyMappingDef`] — every binding here flows verbatim
    /// into the merged `OntologyIR.object_mappings` on commit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_mappings: Vec<PropertyMappingDef>,
    /// FK / bridge / computed / federated edge bindings. Each entry
    /// names the target [`EdgeTypeId`] and the backing
    /// [`LinkMappingKind`] payload — the same shape `LinkMappingDef`
    /// carries, minus the wrapping object so a single artifact can
    /// emit several edges off the same source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_mappings: Vec<EdgeMapping>,
    /// Ambiguities the LLM flagged for operator review. Empty
    /// when the analysis was unambiguous.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<OpenQuestion>,
    /// Prompt + model + author trail.
    pub provenance: ArtifactProvenance,
    /// UTC timestamp the artifact was created. Set by the store
    /// layer on insert; serialised verbatim on read.
    pub created_at: DateTime<Utc>,
    /// User id of the operator who triggered the design action.
    pub created_by: String,
}

/// One edge binding inside an artifact. Mirrors
/// [`crate::mapping::LinkMappingDef`] minus the surrounding object —
/// a single artifact emits several edges off the same source so the
/// per-edge `LinkMappingDef.id` is decided at merge time, not here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct EdgeMapping {
    pub edge_type_id: EdgeTypeId,
    pub kind: LinkMappingKind,
    /// Optional human-readable label for the edge, displayed in
    /// the design review surface. Localised so a bilingual
    /// deployment doesn't have to invent a parallel translation
    /// store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<LocalizedText>,
}

/// One ambiguity the LLM flagged during analysis. Operators
/// resolve these on the design review surface; resolved ones
/// don't carry forward to the next artifact (a fresh analyse
/// re-derives the question set against the new schema hash).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct OpenQuestion {
    /// Stable id within the artifact. Used by the review surface
    /// to attach an operator decision.
    pub id: String,
    /// Where the question is anchored in the source — the column
    /// ref the operator should look at when deciding. `None` when
    /// the question is schema-wide (e.g. "no obvious primary
    /// table").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<ColumnRef>,
    /// Human-readable question text.
    pub message: LocalizedText,
    /// Candidate answers the LLM proposed. Operator picks one or
    /// writes their own; the next analyse pass uses the resolution
    /// as a hint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<LocalizedText>,
}

/// Authorship + reproducibility envelope on every artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ArtifactProvenance {
    /// `prompt_templates.id` of the prompt that drove the design
    /// action.
    pub prompt_id: String,
    /// Semver-ish version string of the prompt at authoring time.
    pub prompt_version: String,
    /// Model identifier (`anthropic:claude-sonnet-4-6`,
    /// `openai:gpt-5`, …). Stable across replays so the
    /// reproducibility signal under
    /// `quality_signal::reproducibility` keeps its meaning.
    pub model_id: String,
    /// Free-form, opaque parameters useful for replay /
    /// debugging — temperature, seed, knobs the prompt uses.
    /// `BTreeMap` for stable serialisation order so the hash is
    /// deterministic.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

impl SourceMappingArtifact {
    /// Derive an artifact from a freshly-designed `OntologyIR` and
    /// the `SourceSchema` it was authored against.
    ///
    /// Walks `ontology.object_mappings()` for the named source and
    /// folds every per-property binding into `property_mappings`;
    /// walks `ontology.link_mappings()` for the same source and
    /// captures FK / Bridge / Computed / Federated edge bindings into
    /// `edge_mappings`. The schema hash is computed from the
    /// canonicalised `SourceSchema` so two introspection runs against
    /// the same physical source collapse to the same hash.
    ///
    /// `created_at` is stamped at call-time. The store layer's
    /// content-addressed unique constraint
    /// `(workspace_id, source_id, schema_snapshot_hash, content_hash)`
    /// dedupes a re-emit of the same body — `created_at` is excluded
    /// from `content_hash` (the body hash) by virtue of being on the
    /// outer struct so a replay collapses to the existing row.
    ///
    /// `id` is minted from the schema-snapshot hash + a 12-character
    /// short-hash of the body so a content-addressed replay produces
    /// the same id; the store layer does not depend on this — the
    /// `ON CONFLICT DO NOTHING` path absorbs duplicate inserts — but
    /// stable ids make logs and audit links deterministic.
    pub fn derive_from_design(
        ontology: &OntologyIR,
        source_id: &SourceId,
        source_schema: &SourceSchema,
        provenance: ArtifactProvenance,
        created_by: impl Into<String>,
    ) -> Self {
        let schema_snapshot_hash = source_schema.canonical_hash();

        let property_mappings: Vec<PropertyMappingDef> = ontology
            .object_mappings()
            .iter()
            .filter(|om| om.source_id == *source_id)
            .flat_map(|om| om.property_mappings.iter().cloned())
            .collect();

        let edge_mappings: Vec<EdgeMapping> = ontology
            .link_mappings()
            .iter()
            .filter(|lm| {
                lm.source_endpoint.source_id == *source_id
                    || lm.target_endpoint.source_id == *source_id
            })
            .map(|lm| EdgeMapping {
                edge_type_id: lm.edge_type_id.clone(),
                kind: lm.kind.clone(),
                label: ontology
                    .edge_by_id(lm.edge_type_id.as_str())
                    .map(|et| et.display_name.clone()),
            })
            .collect();

        let created_at = Utc::now();

        // Mint a stable, content-addressed id. Body hash is computed
        // pre-self-construction over the structural fields so the id
        // is reproducible across replays of the same design call.
        let mut probe = Self {
            id: SourceMappingArtifactId::new("placeholder"),
            source_id: source_id.clone(),
            schema_snapshot_hash: schema_snapshot_hash.clone(),
            property_mappings: property_mappings.clone(),
            edge_mappings: edge_mappings.clone(),
            open_questions: Vec::new(),
            provenance: provenance.clone(),
            created_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now),
            created_by: String::new(),
        };
        let body_hash = probe.content_hash();
        let id = format!(
            "sma-{}-{}",
            short_hash(&schema_snapshot_hash),
            short_hash(&body_hash),
        );
        probe.id = SourceMappingArtifactId::new(&id);
        probe.created_at = created_at;
        probe.created_by = created_by.into();
        probe
    }

    /// Compute the canonical content hash for an artifact body —
    /// the same hash the store layer dedupes on at insert. Stable
    /// across whitespace differences in the source JSON because
    /// it operates on the deserialised structure.
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        // `serde_json::to_string` produces canonical-enough output
        // for hashing — fields are emitted in struct-declaration
        // order, BTreeMap keys are sorted, and `Vec` order is
        // significant by design (operators ordered the questions).
        let bytes = match serde_json::to_vec(self) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        hex_encode(&digest)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // `String`'s fmt::Write impl is infallible — silently
        // swallowing the result is the idiomatic pattern.
        write!(&mut out, "{b:02x}").ok();
    }
    out
}

/// Truncate a hex hash to 12 characters — enough entropy
/// (48 bits) for practical id uniqueness at our scale, short
/// enough to keep ids human-readable in audit logs.
fn short_hash(hash: &str) -> &str {
    let len = hash.len().min(12);
    &hash[..len]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::{PropertyLocation, PropertyTransform};
    use ox_core::property_key::PropertyKey;

    fn fixture() -> SourceMappingArtifact {
        SourceMappingArtifact {
            id: "sma-1".into(),
            source_id: "pg-main".into(),
            schema_snapshot_hash: "deadbeef".into(),
            property_mappings: vec![PropertyMappingDef {
                property_id: "p-id".into(),
                property_key: PropertyKey::new("id").unwrap(),
                location: PropertyLocation::Column(ColumnRef {
                    column: "customer_id".into(),
                    relation: "public.customers".into(),
                }),
                transform: PropertyTransform::Identity,
                concept_map_id: None,
            }],
            edge_mappings: Vec::new(),
            open_questions: Vec::new(),
            provenance: ArtifactProvenance {
                prompt_id: "design.standard".into(),
                prompt_version: "0.4.2".into(),
                model_id: "anthropic:claude-sonnet-4-6".into(),
                params: BTreeMap::new(),
            },
            created_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            created_by: "user-1".into(),
        }
    }

    #[test]
    fn content_hash_is_stable_across_clones() {
        let a = fixture();
        let b = a.clone();
        assert_eq!(a.content_hash(), b.content_hash());
        assert_eq!(a.content_hash().len(), 64);
    }

    #[test]
    fn content_hash_changes_when_body_changes() {
        let a = fixture();
        let mut b = a.clone();
        b.created_by = "user-2".into();
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn content_hash_is_hex_only() {
        let a = fixture();
        let h = a.content_hash();
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    mod derive_from_design {
        use super::*;
        use crate::ir::{NodeTypeId, OntologyIR};
        use crate::mapping::{
            CacheHintKind, EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef,
            LinkMappingId, ObjectMappingDef, ObjectMappingId, SourceRelationKind,
        };
        use crate::test_fixtures;
        use ox_core::source_schema::{SourceColumnDef, SourceSchema, SourceTableDef};

        fn ontology_with_user_node_and_mapping(source: &str) -> OntologyIR {
            let mut ir = test_fixtures::sample_user_ontology();
            // Find one node type to anchor the mapping on. The fixture
            // ships at least one node type by construction.
            let nt: NodeTypeId = ir.node_types()[0].id.clone();

            let om = ObjectMappingDef {
                id: ObjectMappingId::new("om-1"),
                node_type_id: nt,
                source_id: SourceId::new(source),
                relation: "users".into(),
                relation_kind: SourceRelationKind::default(),
                primary_key_columns: Vec::new(),
                row_filter: None,
                property_mappings: vec![PropertyMappingDef {
                    property_id: "p-id".into(),
                    property_key: PropertyKey::new("id").unwrap(),
                    location: PropertyLocation::Column(ColumnRef {
                        column: "id".into(),
                        relation: "users".into(),
                    }),
                    transform: PropertyTransform::Identity,
                    concept_map_id: None,
                }],
                workspace_scope: None,
                precedence: u8::MAX,
                valid_from: None,
                valid_to: None,
                cache_hint: CacheHintKind::default(),
            };
            ir.add_object_mapping(om).expect("add_object_mapping");
            ir
        }

        fn schema_users() -> SourceSchema {
            SourceSchema {
                source_type: "postgresql".into(),
                tables: vec![SourceTableDef {
                    name: "users".into(),
                    columns: vec![SourceColumnDef {
                        name: "id".into(),
                        data_type: "uuid".into(),
                        nullable: false,
                    }],
                    primary_key: vec!["id".into()],
                }],
                foreign_keys: vec![],
            }
        }

        fn provenance() -> ArtifactProvenance {
            ArtifactProvenance {
                prompt_id: "design_ontology".into(),
                prompt_version: "1.0.0".into(),
                model_id: "anthropic:claude-sonnet-4-6".into(),
                params: BTreeMap::new(),
            }
        }

        #[test]
        fn captures_property_bindings_from_canonical_mapping_slice() {
            let ir = ontology_with_user_node_and_mapping("pg-main");
            let artifact = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &schema_users(),
                provenance(),
                "user-1",
            );
            assert_eq!(artifact.property_mappings.len(), 1);
            assert_eq!(artifact.property_mappings[0].property_id.as_str(), "p-id");
            assert_eq!(artifact.source_id.as_str(), "pg-main");
            assert_eq!(artifact.created_by, "user-1");
        }

        #[test]
        fn schema_hash_matches_canonical_hash() {
            let ir = ontology_with_user_node_and_mapping("pg-main");
            let schema = schema_users();
            let artifact = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &schema,
                provenance(),
                "user-1",
            );
            assert_eq!(artifact.schema_snapshot_hash, schema.canonical_hash());
        }

        #[test]
        fn id_is_deterministic_for_same_inputs() {
            let ir = ontology_with_user_node_and_mapping("pg-main");
            let schema = schema_users();
            let a = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &schema,
                provenance(),
                "user-1",
            );
            let b = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &schema,
                provenance(),
                "user-1",
            );
            assert_eq!(a.id, b.id);
        }

        #[test]
        fn id_differs_when_schema_changes() {
            let ir = ontology_with_user_node_and_mapping("pg-main");
            let s1 = schema_users();
            let mut s2 = s1.clone();
            s2.tables[0].columns.push(SourceColumnDef {
                name: "email".into(),
                data_type: "text".into(),
                nullable: true,
            });
            let a = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &s1,
                provenance(),
                "user-1",
            );
            let b = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &s2,
                provenance(),
                "user-1",
            );
            assert_ne!(a.id, b.id);
            assert_ne!(a.schema_snapshot_hash, b.schema_snapshot_hash);
        }

        #[test]
        fn ignores_object_mappings_from_other_sources() {
            let ir = ontology_with_user_node_and_mapping("pg-main");
            let artifact = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-other"),
                &schema_users(),
                provenance(),
                "user-1",
            );
            assert!(artifact.property_mappings.is_empty());
        }

        #[test]
        fn captures_link_mapping_for_target_endpoint_in_source() {
            let mut ir = ontology_with_user_node_and_mapping("pg-main");

            // Need at least one edge to bind a link mapping to.
            let edge_id = ir.edge_types()[0].id.clone();

            let lm = LinkMappingDef {
                id: LinkMappingId::new("lm-1"),
                edge_type_id: edge_id.clone(),
                kind: LinkMappingKind::ForeignKey {
                    source_column: ColumnRef {
                        column: "user_id".into(),
                        relation: "orders".into(),
                    },
                    target_column: ColumnRef {
                        column: "id".into(),
                        relation: "users".into(),
                    },
                },
                source_endpoint: EndpointRef {
                    source_id: SourceId::new("pg-main"),
                    relation: "orders".into(),
                    key_columns: vec!["user_id".into()],
                },
                target_endpoint: EndpointRef {
                    source_id: SourceId::new("pg-main"),
                    relation: "users".into(),
                    key_columns: vec!["id".into()],
                },
                join_cost_hint: JoinCostHint::Indexed,
                precedence: 100,
                cardinality: LinkCardinality::ManyToOne,
            };
            ir.add_link_mapping(lm).expect("add_link_mapping");

            let artifact = SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new("pg-main"),
                &schema_users(),
                provenance(),
                "user-1",
            );
            assert_eq!(artifact.edge_mappings.len(), 1);
            assert_eq!(artifact.edge_mappings[0].edge_type_id, edge_id);
        }
    }
}

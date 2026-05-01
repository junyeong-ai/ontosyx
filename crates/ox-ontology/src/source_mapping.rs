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
    /// SHA-256 of the rendered prompt body — system prompt + user
    /// prompt with every variable interpolated, exactly as the
    /// LLM saw it (ADR-0029). Bumps automatically when an admin
    /// edits the DB row backing `prompt_id` / `prompt_version`
    /// without bumping the version, so a replay against the same
    /// `(prompt_id, prompt_version)` pair surfaces the divergence
    /// instead of silently re-using the prior cache entry. Empty
    /// string when the artifact pre-dates the field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_render_hash: String,
}

impl ArtifactProvenance {
    /// Compute the canonical render hash for a rendered prompt
    /// body. Same input → same hash, deterministic across hosts.
    /// SHA-256 hex (lowercase) so the value round-trips through
    /// JSON / database columns unchanged.
    ///
    /// Callers compose the input from the system + user prompt
    /// after every variable interpolation has resolved — pre-
    /// interpolation snapshots would defeat the gate's purpose.
    pub fn compute_prompt_render_hash(rendered: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(rendered.as_bytes());
        format!("{:x}", hasher.finalize())
    }
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
    /// dedupes a re-emit of the same body — `created_at` /
    /// `created_by` / `id` are deliberately excluded from
    /// [`Self::content_hash`] so a replay against the same
    /// `(source, schema, mappings, provenance)` tuple collapses to
    /// the existing row regardless of clock or operator.
    ///
    /// `id` is minted as `sma-{schema-hash-12}-{body-hash-12}` from
    /// the same content hash the store dedupes on, so a replay
    /// produces the same id deterministically. Content-addressed:
    /// audit links + logs reference the same id across re-runs.
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

        let body = ContentBody {
            source_id,
            schema_snapshot_hash: &schema_snapshot_hash,
            property_mappings: &property_mappings,
            edge_mappings: &edge_mappings,
            provenance: &provenance,
        };
        let body_hash = body.hash();
        let id = SourceMappingArtifactId::new(format!(
            "sma-{}-{}",
            short_hash(&schema_snapshot_hash),
            short_hash(&body_hash),
        ));

        Self {
            id,
            source_id: source_id.clone(),
            schema_snapshot_hash,
            property_mappings,
            edge_mappings,
            open_questions: Vec::new(),
            provenance,
            created_at: Utc::now(),
            created_by: created_by.into(),
        }
    }

    /// Hash of the artifact's *content-addressable* fields — the
    /// same value the store layer's unique constraint dedupes on,
    /// and the same value [`Self::derive_from_design`] derives the
    /// `id` short-hash from.
    ///
    /// Deliberately excludes `id` (would create a circular
    /// dependency on hash-of-the-hash), `created_at` (clock-stamped
    /// at insert, not part of the design decision), `created_by`
    /// (operator identity, recorded but not part of "what mapping
    /// was authored"), and `open_questions` (operator-mutable
    /// post-creation; flipping a question's resolved state must not
    /// orphan the artifact).
    pub fn content_hash(&self) -> String {
        ContentBody {
            source_id: &self.source_id,
            schema_snapshot_hash: &self.schema_snapshot_hash,
            property_mappings: &self.property_mappings,
            edge_mappings: &self.edge_mappings,
            provenance: &self.provenance,
        }
        .hash()
    }
}

/// Borrowed view over the artifact fields that participate in the
/// content hash. Lives next to [`SourceMappingArtifact`] so any
/// future field addition has to choose explicitly: include in the
/// hash (and add here) or exclude (and document why above).
///
/// `Serialize` is derived in struct-declaration order; `serde_json`
/// emits fields in that order so the hash is deterministic across
/// platforms.
#[derive(Serialize)]
struct ContentBody<'a> {
    source_id: &'a SourceId,
    schema_snapshot_hash: &'a str,
    property_mappings: &'a [PropertyMappingDef],
    edge_mappings: &'a [EdgeMapping],
    provenance: &'a ArtifactProvenance,
}

impl ContentBody<'_> {
    fn hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let bytes = match serde_json::to_vec(self) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hex_encode(&hasher.finalize())
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
                prompt_render_hash: String::new(),
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
    fn content_hash_is_pinned_against_fixture() {
        // Pinned hash protects against silent drift: any change to
        // `ContentBody`'s field order, serde rename, or to a nested
        // type's `Serialize` impl shifts this value and the test
        // fails loudly. When that happens, intentionally update the
        // expected hash AND publish a migration note for any
        // dependent stores that compared against persisted rows.
        //
        // Computed once against the canonical `fixture()` body
        // (1 property mapping, no edges, no open questions, fixed
        // provenance, fixed created_at). When an intentional change
        // to `ContentBody` or one of its nested types' `Serialize`
        // impl lands, this assertion fails — recompute the expected
        // value and publish a migration note for any dependent
        // stores that compared against persisted rows.
        const PINNED_HASH: &str =
            "759cd9203503dd46e490f2040cbd1f712087c56f6b17cecf805aba6cfd963937";
        assert_eq!(
            fixture().content_hash(),
            PINNED_HASH,
            "content_hash drift detected — investigate ContentBody field \
             order, serde renames, or nested-type Serialize changes"
        );
    }

    #[test]
    fn content_hash_changes_when_prompt_render_hash_changes() {
        // Bumping the render hash (e.g., admin edited the DB-backing
        // prompt without bumping `prompt_version`) must shift the
        // artifact's content hash so the prior cached row is not
        // reused.
        let a = fixture();
        let mut b = a.clone();
        b.provenance.prompt_render_hash = "deadbeef".to_string();
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn compute_prompt_render_hash_is_deterministic() {
        let h1 = ArtifactProvenance::compute_prompt_render_hash("system\n\nuser");
        let h2 = ArtifactProvenance::compute_prompt_render_hash("system\n\nuser");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert_ne!(
            h1,
            ArtifactProvenance::compute_prompt_render_hash("system\n\nDIFFERENT user")
        );
    }

    #[test]
    fn content_hash_changes_when_property_mappings_change() {
        let a = fixture();
        let mut b = a.clone();
        b.property_mappings[0].property_id = "p-other".into();
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn content_hash_is_invariant_to_transient_fields() {
        // Replays of the same design call differ in `created_at`,
        // `created_by`, and the minted `id` — none of those are
        // mapping decisions, so the hash must stay stable.
        let a = fixture();
        let mut b = a.clone();
        b.id = "sma-different-id".into();
        b.created_at = DateTime::<Utc>::from_timestamp(2_000_000_000, 0).unwrap();
        b.created_by = "user-2".into();
        b.open_questions.push(OpenQuestion {
            id: "q-1".into(),
            anchor: None,
            message: LocalizedText::new("does this column actually identify the customer?"),
            options: Vec::new(),
        });
        assert_eq!(
            a.content_hash(),
            b.content_hash(),
            "transient fields (id, created_at, created_by, open_questions) \
             must not affect the dedup hash"
        );
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
                prompt_render_hash: String::new(),
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

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

use crate::ir::EdgeTypeId;
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
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
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
}

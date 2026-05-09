//! Verified question → QueryIR bank.
//!
//! ## Why a typed verified-query store
//!
//! Vanna.AI's killer pattern is `train(question, sql)` — operator-
//! validated `(natural-language question, structured query)` pairs
//! land in a vector store, RAG retrieves the top-k most-similar
//! priors at NL→SQL time, the LLM gets them as ICL exemplars. The
//! result: dramatic accuracy lift + cost reduction (the LLM
//! anchors against a working pattern instead of inferring from
//! schema alone).
//!
//! Pre-Φ11 Ontosyx had `KnowledgeStore` for failure-driven
//! corrections (RecoveryDetectionHook auto-records "Q failed → Q
//! corrected" pairs) but no positive-example bank. The
//! `EvaluationDataset` surface stores golden Q→IR pairs but
//! evaluation-side, not for runtime ICL injection.
//!
//! [`VerifiedQueryDef`] closes that gap: workspace-scoped
//! collection of operator-promoted verified Q→IR pairs that the
//! Brain reads as ICL exemplars at translate-query time.
//!
//! ## Naming
//!
//! `Def` suffix mirrors the rest of the IR-adjacent typed surface
//! (`ConceptDef`, `RuleDef`, …). Verified queries are *not* IR
//! collection members — they live alongside the ontology, not
//! inside any committed ontology version snapshot, so the
//! collection is not extracted via `IrCollection` / Level-2
//! content-addressed storage. The `Def` suffix is about typed-
//! identity ergonomics, not IR membership.
//!
//! ## Cross-version durability
//!
//! Verified queries persist across ontology version commits. A
//! schema edit may render a verified query stale (the IR
//! references an entity that no longer exists). The
//! [`VerifiedQueryStatus::Stale`] state tracks that — a future
//! cron sweeps committed ontology versions against verified-query
//! IRs and flips the status when the IR no longer validates.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AgentRef;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`VerifiedQueryDef`]. Workspace-
    /// scoped per natural key `(workspace_id, question_hash)`.
    VerifiedQueryId
);

/// Operator-validated `(question, IR)` pair the Brain retrieves
/// at translate-query time.
///
/// Workspace-scoped per `(workspace_id, question_hash)` UNIQUE
/// — the same question authored twice collapses to one row, so
/// re-promoting an already-verified query is idempotent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct VerifiedQueryDef {
    pub id: VerifiedQueryId,
    pub workspace_id: Uuid,
    /// Natural-language question the verified IR answers. Free
    /// text the operator typed or pulled from a chat session.
    pub question: String,
    /// SHA-256 of the canonicalised question
    /// (`canonicalize_question`). UPSERT collapses duplicate
    /// authorings on this field — the same question worded
    /// identically (after trim + lowercase + collapse-whitespace)
    /// hashes the same.
    pub question_hash: String,
    /// `QueryIR` JSON. Stored as `serde_json::Value` so this
    /// layer does not pull `ox-query-ir` (the layering arrow
    /// `ox-core ← ox-ontology ← ox-query-ir` stays); consumers
    /// (Brain, Agent) deserialise via the typed shape.
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub query_ir: serde_json::Value,
    /// Complexity bucket — gates ICL injection. Trivial queries
    /// (single label match, no joins) are intentionally **not**
    /// injected as exemplars: they carry too little structural
    /// signal and risk anchoring the LLM on a degenerate pattern
    /// that doesn't generalise. ontive's experience: trivial
    /// few-shots produced over-literal outputs on novel
    /// questions.
    pub complexity_class: ComplexityClass,
    pub status: VerifiedQueryStatus,
    /// Who promoted the query. `User` for chat-driven
    /// SaveAsVqr; `Service` for cron-extracted patterns;
    /// `LlmModel` is forbidden — verified queries must have
    /// human / service review.
    pub author: AgentRef,
    /// Optional operator note. Renders on the verified-query
    /// admin surface; not included in ICL exemplar bodies (the
    /// Brain only injects the Q + IR pair, not the reviewer's
    /// commentary).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub verified_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Φ11.5 — optional dense embedding for semantic NN retrieval.
    ///
    /// `None` when the embedder hadn't been attached at promote
    /// time (cold-start) or when the dimension didn't match the
    /// schema (the column is `vector(1024)` so a mismatched batch
    /// can't enter the bank). The Brain's ICL retriever falls
    /// back to the trigram path when the row has no embedding,
    /// so cold rows still surface — they just don't benefit from
    /// the paraphrase-recall lift.
    ///
    /// Skipped on the wire when absent so admin / list endpoints
    /// don't ship 4 KB of f32s per row when the FE only needs
    /// the textual surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<f32>>)]
    pub embedding: Option<Vec<f32>>,
    /// Application-side morphological tokenisation of
    /// `question` (workspace tokenizer + user dict). The DB
    /// derives `searchable_tsv` from this column; index-time
    /// (promote) + query-time (Brain ICL retrieval) thread the
    /// same workspace tokenizer so recall stays consistent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tokenized_text: String,
    /// Workspace user-dict fingerprint that produced
    /// `tokenized_text`. Diff against the workspace's current
    /// fingerprint identifies stale rows for the backfill task.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tokenizer_dict_fingerprint: String,
}

/// Complexity bucket pinned at promote time.
///
/// Closed enum — every variant maps to a distinct ICL inclusion
/// rule on the Brain side. Adding a class is one variant + one
/// arm in the Brain's exemplar-selection policy.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ComplexityClass {
    /// Single label match, optionally with a single property
    /// equality. Intentionally **excluded** from ICL exemplar
    /// retrieval — too little structural signal to generalise.
    /// Stored anyway so the operator surface can show "X
    /// trivial verifications + Y ICL-eligible".
    Trivial,
    /// 1-hop traversal, 1-2 property constraints. The most
    /// common ICL exemplar shape.
    Simple,
    /// 2-3 hops, multiple constraints, simple aggregation.
    Composite,
    /// Multi-hop, complex aggregations, conditional predicates.
    /// The exemplar shape that lifts NL→IR accuracy on
    /// long-horizon questions; rarer in the bank but high
    /// per-row value.
    Complex,
}

impl ComplexityClass {
    /// `false` for [`Self::Trivial`], `true` for the other
    /// classes — the Brain's ICL retrieval gate.
    pub fn is_icl_eligible(self) -> bool {
        !matches!(self, ComplexityClass::Trivial)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ComplexityClass::Trivial => "trivial",
            ComplexityClass::Simple => "simple",
            ComplexityClass::Composite => "composite",
            ComplexityClass::Complex => "complex",
        }
    }

    pub fn from_wire_str(s: &str) -> Option<Self> {
        Some(match s {
            "trivial" => Self::Trivial,
            "simple" => Self::Simple,
            "composite" => Self::Composite,
            "complex" => Self::Complex,
            _ => return None,
        })
    }
}

/// Lifecycle state of a verified query.
///
/// `Verified` is the steady state — eligible for ICL retrieval
/// (subject to [`ComplexityClass::is_icl_eligible`]). Other
/// states park the row out of the retrieval pool but keep the
/// audit trail.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum VerifiedQueryStatus {
    /// Operator-approved, eligible for ICL retrieval. Default
    /// post-promotion state.
    Verified,
    /// Submitted via `SaveAsVqr` (chat side) or auto-extracted by
    /// a CIC proposer; awaits operator review on the
    /// `verified-queries` admin surface. Not yet eligible for
    /// retrieval.
    UnderReview,
    /// Operator deprecated. Not eligible for retrieval; row kept
    /// for audit lineage.
    Deprecated,
    /// Schema drift detected — the IR references an entity that
    /// no longer exists in the workspace's canonical ontology.
    /// Set by the verified-query freshness cron (future). Not
    /// eligible for retrieval; the operator surface flags the
    /// row for re-validation.
    Stale,
}

impl VerifiedQueryStatus {
    pub fn is_retrievable(self) -> bool {
        matches!(self, VerifiedQueryStatus::Verified)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            VerifiedQueryStatus::Verified => "verified",
            VerifiedQueryStatus::UnderReview => "under_review",
            VerifiedQueryStatus::Deprecated => "deprecated",
            VerifiedQueryStatus::Stale => "stale",
        }
    }

    pub fn from_wire_str(s: &str) -> Option<Self> {
        Some(match s {
            "verified" => Self::Verified,
            "under_review" => Self::UnderReview,
            "deprecated" => Self::Deprecated,
            "stale" => Self::Stale,
            _ => return None,
        })
    }
}

/// Canonicalise a question for hashing — trim leading/trailing
/// whitespace, lowercase, collapse internal whitespace runs to a
/// single space. Two operators promoting the same question with
/// different spacing / casing land on the same row.
pub fn canonicalize_question(question: &str) -> String {
    let lower = question.trim().to_lowercase();
    let mut canonical = String::with_capacity(lower.len());
    let mut prev_was_space = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            if !prev_was_space {
                canonical.push(' ');
                prev_was_space = true;
            }
        } else {
            canonical.push(c);
            prev_was_space = false;
        }
    }
    canonical
}

/// SHA-256 of the canonicalised question, lowercase hex. Stable
/// across re-runs and platform versions.
pub fn question_hash(question: &str) -> String {
    let canonical = canonicalize_question(question);
    let digest = Sha256::digest(canonical.as_bytes());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(question: &str, complexity: ComplexityClass) -> VerifiedQueryDef {
        let now = Utc::now();
        let q_hash = question_hash(question);
        VerifiedQueryDef {
            id: VerifiedQueryId::new(format!("vq-{q_hash}").chars().take(40).collect::<String>()),
            workspace_id: Uuid::nil(),
            question: question.into(),
            question_hash: q_hash,
            query_ir: serde_json::json!({"kind": "match", "labels": ["Customer"]}),
            complexity_class: complexity,
            status: VerifiedQueryStatus::Verified,
            author: AgentRef::User {
                user_id: "u-1".into(),
            },
            description: String::new(),
            verified_at: now,
            updated_at: now,
            embedding: None,
            tokenized_text: String::new(),
            tokenizer_dict_fingerprint: String::new(),
        }
    }

    #[test]
    fn canonicalize_question_collapses_whitespace_and_lowercases() {
        assert_eq!(
            canonicalize_question("  How Many   Customers Are Active?  "),
            "how many customers are active?"
        );
    }

    #[test]
    fn canonicalize_treats_tabs_newlines_as_spaces() {
        assert_eq!(
            canonicalize_question("How\tmany\ncustomers?"),
            "how many customers?"
        );
    }

    #[test]
    fn question_hash_stable_across_cosmetic_variations() {
        let a = question_hash("How many customers?");
        let b = question_hash("how many   customers?");
        let c = question_hash("  How many customers?  \n");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn question_hash_distinct_for_different_questions() {
        let a = question_hash("How many customers?");
        let b = question_hash("How many orders?");
        assert_ne!(a, b);
    }

    #[test]
    fn question_hash_is_64_char_lowercase_hex() {
        let h = question_hash("anything");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn complexity_class_icl_gate_excludes_trivial() {
        assert!(!ComplexityClass::Trivial.is_icl_eligible());
        assert!(ComplexityClass::Simple.is_icl_eligible());
        assert!(ComplexityClass::Composite.is_icl_eligible());
        assert!(ComplexityClass::Complex.is_icl_eligible());
    }

    #[test]
    fn complexity_class_wire_strings_round_trip() {
        for c in [
            ComplexityClass::Trivial,
            ComplexityClass::Simple,
            ComplexityClass::Composite,
            ComplexityClass::Complex,
        ] {
            let wire = c.as_str();
            assert_eq!(ComplexityClass::from_wire_str(wire), Some(c));
        }
    }

    #[test]
    fn status_retrievable_only_for_verified() {
        assert!(VerifiedQueryStatus::Verified.is_retrievable());
        assert!(!VerifiedQueryStatus::UnderReview.is_retrievable());
        assert!(!VerifiedQueryStatus::Deprecated.is_retrievable());
        assert!(!VerifiedQueryStatus::Stale.is_retrievable());
    }

    #[test]
    fn status_wire_strings_round_trip() {
        for s in [
            VerifiedQueryStatus::Verified,
            VerifiedQueryStatus::UnderReview,
            VerifiedQueryStatus::Deprecated,
            VerifiedQueryStatus::Stale,
        ] {
            let wire = s.as_str();
            assert_eq!(VerifiedQueryStatus::from_wire_str(wire), Some(s));
        }
    }

    #[test]
    fn verified_query_def_round_trips_through_json() {
        let q = sample("How many active customers?", ComplexityClass::Simple);
        let v = serde_json::to_value(&q).unwrap();
        let back: VerifiedQueryDef = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, q.id);
        assert_eq!(back.question, q.question);
        assert_eq!(back.question_hash, q.question_hash);
        assert_eq!(back.complexity_class, q.complexity_class);
        assert_eq!(back.status, q.status);
    }
}

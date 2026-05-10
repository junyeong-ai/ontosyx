//! [`EvaluationFingerprint`] — typed reproducibility bundle every
//! evaluation run pins.
//!
//! ## Why one struct, not five columns
//!
//! Reproducibility is not a single field: it is a *combination*.
//! "this run produced these numbers because we sent prompt template
//! `evaluation_judge` v1.4, rendered against ontology version X,
//! routed retrieval through profile P, decoded at temperature T,
//! over dataset D." Spreading those across run-level columns made
//! every one optional ("ad-hoc runs have no dataset", "draft-stage
//! runs have no ontology version"); the silent loss path was a run
//! created without a pin → schema drift → six months later the
//! score is uninterpretable.
//!
//! A closed [`EvaluationFingerprint`] struct + a SHA-256 digest
//! forces every call site to pin the full combination at construction
//! time. Two runs are "the same configuration" iff their digests are
//! equal — single equality token, no fanout JOINs.
//!
//! ## Extensibility
//!
//! New dimensions land as additional fields with `#[serde(default,
//! skip_serializing_if = "Option::is_none")]` and ride on the JSONB
//! `fingerprint_components` column without a schema migration. Old
//! runs keep their digests stable because their JSON shape doesn't
//! change; new runs with the new field carry a different digest set.
//!
//! ## Layering
//!
//! Lives in `ox-ontology` because the fingerprint is a domain
//! contract — `ox-store` persists it, `ox-api` exposes it,
//! `ox-brain` constructs it at evaluation kickoff. None of those
//! layers may invent their own version of the bundle.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};

ox_core::define_id_newtype!(
    /// Stable identifier for an LLM model + version. Wire form:
    /// provider-prefixed version string
    /// (`anthropic/claude-opus-4-7`, `openai/gpt-4o-2024-08-06`).
    /// Replaces the bare `String` previously threaded through the
    /// Brain / evaluation surface so a model id can never be
    /// confused with another stable identifier.
    ModelId
);

ox_core::define_id_newtype!(
    /// Stable identifier for a prompt template — the
    /// `prompts/<name>.toml` filename without the extension. Pairs
    /// with a separate `prompt_template_version` semver field on
    /// the fingerprint so the template id stays stable across
    /// versions.
    PromptTemplateId
);

ox_core::define_id_newtype!(
    /// Stable identifier for a [`crate::retrieval::RetrievalProfile`].
    /// The profile struct itself lands in Φ10 (GraphRAG retrieval as
    /// data); the id type is declared early so [`EvaluationFingerprint`]
    /// can pin the retrieval policy without a forward-declaration churn
    /// when Φ10 lands.
    RetrievalProfileId
);

/// Hexadecimal SHA-256 digest of canonical JSON over a typed
/// configuration block. Used by [`EvaluationFingerprint`] to fold
/// extensible decoding parameters (temperature, top_p, max_tokens,
/// system prompt prefix, …) into a single equality token; new
/// parameters extend the JSONB without forcing a column add.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(transparent)]
pub struct ConfigHash(pub String);

impl ConfigHash {
    /// SHA-256 of the canonical JSON of `value` rendered as a
    /// 64-char lowercase hex string. Two configurations producing
    /// the same canonical bytes hash identically — the digest is
    /// the configuration's identity.
    pub fn from_value(value: &serde_json::Value) -> Self {
        let canonical = crate::storage::canonical_json(value);
        let digest = Sha256::digest(canonical.as_bytes());
        Self(hex::encode(digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ConfigHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The typed bundle every evaluation run pins for reproducibility.
///
/// Every field except the explicitly-optional ones is required —
/// the whole point of the bundle is forcing the call site to
/// answer every reproducibility question at construction time.
/// `Option<T>` fields name dimensions that genuinely do not apply
/// for every run (e.g. deterministic retrieval-only scoring uses
/// no prompt template).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct EvaluationFingerprint {
    /// Committed ontology version the run scored against. Pre-
    /// canonical workspaces commit a draft before evaluating —
    /// drafts cannot be evaluated as-is because the schema may
    /// shift mid-run. The persistence layer enforces ON DELETE
    /// RESTRICT so a snapshot referenced by a run cannot be
    /// garbage-collected.
    pub ontology_version_id: Uuid,
    /// Dataset every case derives from. Drives the case_key
    /// correspondence two runs need to be diff-able. ON DELETE
    /// RESTRICT — a referenced dataset cannot be removed while
    /// runs depend on it.
    pub dataset_id: Uuid,
    /// Model that produced the candidate output. For deterministic
    /// retrieval-only scoring (no LLM invocation), the operator
    /// passes a sentinel id — the digest still pins the choice so
    /// the run is comparable to itself across re-executions.
    pub model_id: ModelId,
    /// Prompt template id (`design_ontology`, `evaluation_judge`,
    /// …). `None` for runs that do not invoke an LLM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_template_id: Option<PromptTemplateId>,
    /// Semver of the prompt template that ran. Pairs 1:1 with
    /// `prompt_template_id` — both `Some` or both `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_template_version: Option<String>,
    /// Hash of decoding configuration (temperature / top_p /
    /// max_tokens / system-prompt prefix / …). Folds extensible
    /// per-call decoding state into a single equality token; new
    /// dimensions extend the hashed JSONB without a schema change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoding_config_hash: Option<ConfigHash>,
    /// Forward-compat slot. Φ10 lands the retrieval policy as a
    /// first-class struct; the id is pinned here for runs from
    /// that phase onward. `None` for Φ8/Φ9-era runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_profile_id: Option<RetrievalProfileId>,
}

impl EvaluationFingerprint {
    /// SHA-256 of canonical JSON over `self`, rendered as a 64-char
    /// lowercase hex string. The single equality token for "two runs
    /// were configured the same way." Persisted alongside the run so
    /// the FE can join two runs by digest without re-fetching the
    /// component bag.
    pub fn digest(&self) -> OxResult<String> {
        let value = serde_json::to_value(self).map_err(|e| OxError::Runtime {
            message: format!("EvaluationFingerprint canonical serialise failed: {e}"),
        })?;
        let canonical = crate::storage::canonical_json(&value);
        let digest = Sha256::digest(canonical.as_bytes());
        Ok(hex::encode(digest))
    }
}

/// Pre-validation request shape for constructing an
/// [`EvaluationFingerprint`]. Carries raw decoding configuration as
/// free-form JSON; the canonicaliser hashes it on the way through.
/// This is the wire shape an HTTP / cron / sampler caller submits;
/// the canonical [`EvaluationFingerprint`] only escapes from
/// `into_fingerprint`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct EvaluationFingerprintInput {
    pub ontology_version_id: uuid::Uuid,
    pub dataset_id: uuid::Uuid,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_template_version: Option<String>,
    /// Free-form decoding configuration — the canonicaliser hashes
    /// it into [`EvaluationFingerprint::decoding_config_hash`]. Two
    /// inputs that differ only in JSON key ordering hash
    /// identically; per-call deterministic replay relies on this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub decoding_config: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_profile_id: Option<String>,
}

impl EvaluationFingerprintInput {
    /// Promote raw inputs into the canonical fingerprint. The hash
    /// of `decoding_config` is computed under canonical JSON so the
    /// caller does not have to pre-canonicalise.
    pub fn into_fingerprint(self) -> EvaluationFingerprint {
        EvaluationFingerprint {
            ontology_version_id: self.ontology_version_id,
            dataset_id: self.dataset_id,
            model_id: ModelId::new(self.model_id),
            prompt_template_id: self.prompt_template_id.map(PromptTemplateId::new),
            prompt_template_version: self.prompt_template_version,
            decoding_config_hash: self.decoding_config.as_ref().map(ConfigHash::from_value),
            retrieval_profile_id: self.retrieval_profile_id.map(RetrievalProfileId::new),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fingerprint() -> EvaluationFingerprint {
        EvaluationFingerprint {
            ontology_version_id: Uuid::nil(),
            dataset_id: Uuid::nil(),
            model_id: ModelId::new("anthropic/claude-opus-4-7"),
            prompt_template_id: Some(PromptTemplateId::new("evaluation_judge")),
            prompt_template_version: Some("1.4.0".into()),
            decoding_config_hash: Some(ConfigHash::from_value(
                &serde_json::json!({"temperature": 0.0, "max_tokens": 2048}),
            )),
            retrieval_profile_id: None,
        }
    }

    #[test]
    fn digest_is_stable_across_serialise_roundtrip() {
        let fp = sample_fingerprint();
        let d1 = fp.digest().unwrap();
        let v = serde_json::to_value(&fp).unwrap();
        let back: EvaluationFingerprint = serde_json::from_value(v).unwrap();
        let d2 = back.digest().unwrap();
        assert_eq!(d1, d2);
    }

    #[test]
    fn digest_changes_on_any_field() {
        let mut a = sample_fingerprint();
        let d_a = a.digest().unwrap();
        a.model_id = ModelId::new("openai/gpt-4o-2024-08-06");
        let d_b = a.digest().unwrap();
        assert_ne!(d_a, d_b, "model_id flip must change digest");
    }

    #[test]
    fn config_hash_is_canonical_under_key_reordering() {
        let h1 = ConfigHash::from_value(&serde_json::json!({"a": 1, "b": 2}));
        let h2 = ConfigHash::from_value(&serde_json::json!({"b": 2, "a": 1}));
        assert_eq!(
            h1, h2,
            "canonical JSON sorts keys → permutation must hash identically"
        );
    }

    #[test]
    fn digest_is_64_char_lowercase_hex() {
        let d = sample_fingerprint().digest().unwrap();
        assert_eq!(d.len(), 64);
        assert!(
            d.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "digest must be 64-char lowercase hex"
        );
    }

    #[test]
    fn optional_fields_omitted_on_wire_when_none() {
        let fp = EvaluationFingerprint {
            ontology_version_id: Uuid::nil(),
            dataset_id: Uuid::nil(),
            model_id: ModelId::new("retrieval-only/scoring"),
            prompt_template_id: None,
            prompt_template_version: None,
            decoding_config_hash: None,
            retrieval_profile_id: None,
        };
        let v = serde_json::to_value(&fp).unwrap();
        assert!(v.get("prompt_template_id").is_none());
        assert!(v.get("prompt_template_version").is_none());
        assert!(v.get("decoding_config_hash").is_none());
        assert!(v.get("retrieval_profile_id").is_none());
    }
}

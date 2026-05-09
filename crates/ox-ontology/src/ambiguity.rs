//! Persistent ambiguity contexts + resolutions.
//!
//! A column whose values cannot be interpreted from schema alone
//! (numeric codes, opaque short codes, overloaded names) becomes an
//! [`AmbiguityContext`] at source-analysis time. Admins — or the
//! agent — resolve it by attaching an [`AmbiguityResolution`] whose
//! [`AmbiguityMapping`] either enumerates value meanings, points to
//! an existing [`crate::code_system::CodeSystemDef`], or pins a
//! canonical [`crate::concept::ConceptDef`] for overloaded names.
//!
//! The pair is designed as a **closed loop**:
//! 1. Detector emits `AmbiguityContext` rows on every source
//!    analysis (source_hash pinned).
//! 2. Query planner checks the active resolution for each touched
//!    column at execution time.
//! 3. Unresolved hits emit a `QueryDiagnostic` back to the agent /
//!    UI so the next turn can call `resolve_ambiguity`.
//!
//! Old schemas auto-invalidate: changing the source re-hashes the
//! context and any resolution whose `context_id` doesn't match the
//! fresh context is ignored until the admin re-confirms.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::code_system::CodeSystemId;
use crate::concept::ConceptId;
use crate::mapping::refs::{ColumnRef, SourceId};

ox_core::define_id_newtype!(AmbiguityId);
ox_core::define_id_newtype!(AmbiguityResolutionId);

/// A column whose values cannot be interpreted from schema alone.
///
/// Persisted at source-analysis time; one row per
/// `(workspace_id, source_id, relation, column)`. Re-running analysis
/// replaces the row (same natural key), and `detection_source_hash`
/// invalidates stale resolutions that were attached to an earlier
/// schema shape.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
pub struct AmbiguityContext {
    pub id: AmbiguityId,
    pub source_id: SourceId,
    pub column: ColumnRef,
    pub kind: AmbiguityKind,
    /// Distinct-value samples pulled from the source profile.
    /// Order-preserving, de-duplicated, capped at 20 entries.
    #[serde(default)]
    pub sample_values: Vec<String>,
    /// Approximate distinct cardinality reported by the profiler —
    /// `None` when the source adapter couldn't estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct_estimate: Option<u64>,
    /// Did any sample row carry a NULL? Used by the resolver UI to
    /// propose an implicit "missing" value mapping.
    #[serde(default)]
    pub nullable: bool,
    /// Human-readable question the admin / LLM sees. Authored once by
    /// the detector; stable across re-runs so a resolution in flight
    /// doesn't lose its prompt context when the hash rolls.
    pub clarification_prompt: String,
    /// Snapshot hash of the underlying schema + profile fingerprint
    /// used at detection time. Any resolution whose
    /// `context_source_hash` differs is treated as stale.
    pub detection_source_hash: String,
    /// Repo-enrichment hint (ORM enum declaration, migration doc)
    /// when the detector found a matching definition. Advisory only —
    /// the admin must accept / edit / reject it when resolving.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_hint: Option<RepoHint>,
    pub detected_at: DateTime<Utc>,
}

impl AmbiguityContext {
    /// Construct a fresh UUID-backed context. `detected_at` defaults
    /// to now so callers don't have to thread a clock just to record
    /// a context.
    pub fn new(
        source_id: SourceId,
        column: ColumnRef,
        kind: AmbiguityKind,
        sample_values: Vec<String>,
        clarification_prompt: String,
        detection_source_hash: String,
    ) -> Self {
        Self {
            id: AmbiguityId::new(Uuid::new_v4().to_string()),
            source_id,
            column,
            kind,
            sample_values,
            distinct_estimate: None,
            nullable: false,
            clarification_prompt,
            detection_source_hash,
            repo_hint: None,
            detected_at: Utc::now(),
        }
    }

    pub fn with_repo_hint(mut self, hint: RepoHint) -> Self {
        self.repo_hint = Some(hint);
        self
    }

    pub fn with_distinct_estimate(mut self, estimate: u64) -> Self {
        self.distinct_estimate = Some(estimate);
        self
    }

    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }
}

/// Kind of ambiguity observed. Extending this enum is additive — no
/// existing `AmbiguityResolution` becomes invalid when a new variant
/// ships, because resolutions key off `context_id`, not `kind`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AmbiguityKind {
    /// All sampled values parse as integers with low cardinality
    /// (e.g. `1`, `2`, `3`) — classic "coded status" shape.
    NumericCode,
    /// Short uppercase codes mixed with longer human values
    /// (`["N", "Regular", "Town"]`). The shorts are the opaque part.
    OpaqueShortCode,
    /// Same column name across sources with diverging semantics.
    /// Resolution picks which Glossary term this column binds to on
    /// this source specifically.
    OverloadedName,
}

/// Repo-derived hint attached to a detected context. Carries the
/// declaration site so the admin can open the file and confirm.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
pub struct RepoHint {
    /// Pre-formatted `code=label, ...` suggestion from the repo
    /// scan. UI parses this back into `ValueMapEntry`s when the
    /// admin clicks "accept" — storing it as text keeps the
    /// contract robust to changes in the repo scanner output shape.
    pub suggested_values: String,
    /// File where the declaration was found (e.g.
    /// `app/models/order.rb:42`). Caller uses this for a link-back.
    pub source_file: String,
}

/// A resolved interpretation for an [`AmbiguityContext`]. At most one
/// resolution is active per context; creating a new one sets the old
/// one's `supersedes` pointer so the resolution chain is reviewable.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
pub struct AmbiguityResolution {
    pub id: AmbiguityResolutionId,
    pub context_id: AmbiguityId,
    /// Hash of the context at resolution time. A later context whose
    /// `detection_source_hash` diverges is considered stale and the
    /// query planner will re-ask.
    pub context_source_hash: String,
    pub mapping: AmbiguityMapping,
    pub resolved_at: DateTime<Utc>,
    /// User uuid when a human resolved it, or `None` when the agent
    /// auto-resolved via an accepted repo hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by_user_id: Option<Uuid>,
    /// Previous resolution for this context, if any. Enables undo +
    /// audit trail without deleting history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<AmbiguityResolutionId>,
    /// Soft-delete marker. Revoking without replacement returns the
    /// context to "unresolved" state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
}

impl AmbiguityResolution {
    pub fn new(
        context_id: AmbiguityId,
        context_source_hash: String,
        mapping: AmbiguityMapping,
    ) -> Self {
        Self {
            id: AmbiguityResolutionId::new(Uuid::new_v4().to_string()),
            context_id,
            context_source_hash,
            mapping,
            resolved_at: Utc::now(),
            resolved_by_user_id: None,
            supersedes: None,
            revoked_at: None,
        }
    }
}

/// The actual semantic binding the admin / agent chose.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AmbiguityMapping {
    /// Enumerate each raw value with a display + optional definition.
    /// The right choice when the column's value set is narrow and the
    /// admin doesn't want to promote it to a reusable CodeSystem.
    ValueMap { entries: Vec<ValueMapEntry> },
    /// Promote the column to an existing CodeSystem — the column's
    /// raw values are codes within that system. Use this when the
    /// values are already semantically managed elsewhere (e.g. an
    /// ISO-standard status code or an internal coded-value table).
    CodeSystemRef { code_system_id: CodeSystemId },
    /// Pin a canonical concept on this source specifically — the right
    /// move for `OverloadedName` where the same column name means
    /// different things across sources.
    ConceptRef { concept_id: ConceptId },
}

/// Single entry in a [`AmbiguityMapping::ValueMap`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
pub struct ValueMapEntry {
    /// Raw source value as it appears in the column.
    pub value: String,
    /// Human-readable label to show in results / prompts.
    pub display: String,
    /// Longer definition for Glossary-style search. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_column() -> ColumnRef {
        ColumnRef {
            relation: "orders".into(),
            column: "status".into(),
        }
    }

    #[test]
    fn numeric_code_context_round_trips_through_json() {
        let ctx = AmbiguityContext::new(
            SourceId::new("src-oy-pg"),
            example_column(),
            AmbiguityKind::NumericCode,
            vec!["1".into(), "2".into(), "3".into()],
            "Column `orders.status` contains numeric codes …".into(),
            "sha256:abc".into(),
        );
        let j = serde_json::to_value(&ctx).unwrap();
        let back: AmbiguityContext = serde_json::from_value(j).unwrap();
        assert_eq!(back, ctx);
    }

    #[test]
    fn value_map_resolution_round_trips() {
        let r = AmbiguityResolution::new(
            AmbiguityId::new(Uuid::new_v4().to_string()),
            "sha256:abc".into(),
            AmbiguityMapping::ValueMap {
                entries: vec![
                    ValueMapEntry {
                        value: "1".into(),
                        display: "Active".into(),
                        definition: None,
                    },
                    ValueMapEntry {
                        value: "2".into(),
                        display: "Suspended".into(),
                        definition: Some("Locked by fraud check".into()),
                    },
                ],
            },
        );
        let j = serde_json::to_value(&r).unwrap();
        let back: AmbiguityResolution = serde_json::from_value(j).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn code_system_ref_is_a_distinct_variant_on_the_wire() {
        let m = AmbiguityMapping::CodeSystemRef {
            code_system_id: CodeSystemId::new("cs-order-status"),
        };
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"kind\":\"code_system_ref\""));
    }

    #[test]
    fn concept_ref_variant_serialises_distinctly() {
        let m = AmbiguityMapping::ConceptRef {
            concept_id: ConceptId::new("c-vip"),
        };
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"kind\":\"concept_ref\""));
    }

    #[test]
    fn kind_serialises_as_internal_tag_struct() {
        let j = serde_json::to_string(&AmbiguityKind::NumericCode).unwrap();
        assert_eq!(j, r#"{"kind":"numeric_code"}"#);
    }
}

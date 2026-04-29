//! Heuristic-proposal registry — ADR-0023.
//!
//! Every heuristic the platform runs (FK candidate inference,
//! value-set / notation-pattern inference, glossary binding
//! suggestion, reconcile fuzzy match, PII detection, federated-edge
//! candidate proposals) feeds the same shape: a structured
//! `HeuristicProposal` carrying the detector's id, version,
//! confidence, and the evidence the operator needs to confirm or
//! reject. A `HeuristicProvenance` audit row pins the lifecycle
//! event when a proposal is accepted, declined, or expires.
//!
//! Four invariants this module enforces by construction:
//!
//! 1. **No silent IR mutation.** A heuristic never writes to the
//!    canonical IR. It produces a `HeuristicProposal`; the
//!    `OntologyEditor` and admin endpoints route the proposal
//!    through operator review before any IR collection sees the
//!    change. Bypassing the queue is a surface a code reviewer
//!    catches because there is no other way to reach the IR
//!    mutator from a heuristic crate.
//! 2. **Confidence + evidence are first class.** `confidence_bps`
//!    is basis points (0–10_000) so the IR keeps `Eq` / `Hash`
//!    cleanly without float NaN ambiguity, and `evidence` is a
//!    structured `Value` operators can inspect — the heuristic
//!    cannot "trust me" its way into automatic acceptance.
//! 3. **Threshold-below proposals require explicit operator action.**
//!    Auto-accept policy belongs to the consuming surface, but the
//!    proposal carries `auto_accept_threshold_bps` so the surface
//!    cannot accidentally apply a lower bar than the heuristic
//!    author intended.
//! 4. **Audit follows the proposal.** `HeuristicProvenance` records
//!    the decision, who decided, when, and an evidence hash for
//!    cross-check against the original proposal payload. The audit
//!    survives the proposal — even after the proposal is reaped,
//!    the provenance row stays so the rule-origin chain
//!    (`RuleOrigin::ObservedInvariant`, `RuleOrigin::LlmProposed`)
//!    can point at it.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

ox_core::define_id_newtype!(
    /// Stable identifier for one [`HeuristicProposal`].
    HeuristicProposalId
);

ox_core::define_id_newtype!(
    /// Stable identifier for one [`HeuristicProvenance`] audit row.
    HeuristicProvenanceId
);

/// Catalogue of detectors that emit proposals. The string value is
/// wire-stable (audit rows persist it) and identifies which
/// heuristic produced the proposal — the consuming surface uses
/// it to gate threshold policy and to render help links to the
/// detector's documentation.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HeuristicDetectorKind {
    /// Foreign-key candidate inference from `_id` suffix + plural
    /// matching. `crates/ox-source/src/analyzer/fk_inference.rs`.
    ForeignKeyInference,
    /// Low-cardinality columns → ValueSet proposal.
    /// `crates/ox-ontology/src/value_set_inference.rs`.
    ValueSetInference,
    /// Regex-pattern detection over column samples → NotationPattern
    /// proposal. `crates/ox-ontology/src/notation_inference.rs`.
    NotationPatternInference,
    /// Glossary-term ↔ property/type binding suggestions.
    /// `crates/ox-ontology/src/binding_suggestions.rs`.
    GlossaryBindingSuggestion,
    /// Reconcile fuzzy match between an LLM-refined ontology and the
    /// existing baseline. `crates/ox-ontology/src/command/reconcile.rs`.
    ReconcileFuzzyMatch,
    /// PII heuristic redaction in source profile collection.
    /// `crates/ox-source/src/analyzer/pii_scan.rs`.
    PiiHeuristicScan,
    /// Cross-source federated-edge candidate proposals (Phase 3
    /// extension under ADR-0024).
    FederatedEdgeProposal,
    /// Free-form escape so an experimental heuristic can register
    /// without a schema bump. The first invariant still holds: even
    /// a `Custom` detector cannot mutate the IR — every consumer
    /// goes through the proposal queue.
    Custom { detector_id: String },
}

impl HeuristicDetectorKind {
    /// Stable wire identifier — used in audit rows and as the
    /// dedup key inside [`HeuristicProvenance::evidence_hash`].
    pub fn id(&self) -> String {
        match self {
            Self::ForeignKeyInference => "foreign_key_inference".to_string(),
            Self::ValueSetInference => "value_set_inference".to_string(),
            Self::NotationPatternInference => "notation_pattern_inference".to_string(),
            Self::GlossaryBindingSuggestion => "glossary_binding_suggestion".to_string(),
            Self::ReconcileFuzzyMatch => "reconcile_fuzzy_match".to_string(),
            Self::PiiHeuristicScan => "pii_heuristic_scan".to_string(),
            Self::FederatedEdgeProposal => "federated_edge_proposal".to_string(),
            Self::Custom { detector_id } => detector_id.clone(),
        }
    }
}

/// Confidence expressed in basis points (0–10_000). 10_000 = 100%.
/// Basis points keep `Eq` / `Hash` clean — float NaN would infect
/// every container that needs to compare proposals (the proposal
/// queue dedups by `(detector, evidence_hash)` pairs).
///
/// A `ConfidenceBps` is not a percentage exposed to the operator
/// — it is the heuristic's self-rated certainty. Surfaces render
/// it as a confidence chip after dividing by 100; the typed
/// wrapper keeps callers from mistakenly multiplying.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
    JsonSchema, utoipa::ToSchema,
)]
pub struct ConfidenceBps(u32);

impl ConfidenceBps {
    /// Construct from basis points. Values above 10_000 saturate —
    /// a heuristic claiming > 100% confidence is a bug, but
    /// silently capping is safer than panicking on the hot path.
    pub fn from_bps(bps: u32) -> Self {
        Self(bps.min(10_000))
    }

    /// Construct from a 0.0..=1.0 ratio. NaN / negative / above-1.0
    /// inputs all map to a documented sentinel: NaN → 0 (treated
    /// as "no confidence", forcing operator review), out-of-range
    /// values clamp.
    pub fn from_ratio(ratio: f64) -> Self {
        if !ratio.is_finite() || ratio <= 0.0 {
            return Self(0);
        }
        if ratio >= 1.0 {
            return Self(10_000);
        }
        Self((ratio * 10_000.0).round() as u32)
    }

    pub fn as_bps(self) -> u32 {
        self.0
    }

    pub fn as_ratio(self) -> f64 {
        f64::from(self.0) / 10_000.0
    }
}

impl Default for ConfidenceBps {
    /// `0` — heuristics that fail to compute a confidence land at
    /// the bottom of the queue and require explicit operator
    /// action.
    fn default() -> Self {
        Self(0)
    }
}

/// Proposal lifecycle state.
///
/// `Pending` proposals are visible to the review queue. `Accepted`
/// / `Declined` proposals carry a [`HeuristicProvenance`]
/// reference; both are terminal. `Expired` is the cleanup-cron
/// terminal — a proposal whose evidence window rotated without a
/// human decision is surfaced as expired so a future detector run
/// can re-propose against fresh data without bypassing review.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HeuristicProposalState {
    Pending,
    Accepted,
    Declined,
    Expired,
}

/// One pending suggestion from a heuristic detector. Carries
/// everything an operator needs to decide and everything an
/// auto-accept gate needs to short-circuit.
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct HeuristicProposal {
    pub id: HeuristicProposalId,
    pub detector: HeuristicDetectorKind,
    /// Detector implementation version. Bumped by the detector
    /// author when the algorithm changes meaningfully. Auto-accept
    /// policies key on `(detector, version)` so an algorithmic
    /// change doesn't silently re-apply the prior acceptance
    /// threshold to a different surface.
    pub detector_version: u32,
    /// Self-rated confidence (basis points).
    pub confidence: ConfidenceBps,
    /// Optional auto-accept threshold the consuming surface should
    /// honour. `None` means "operator review required regardless of
    /// confidence". Keeping the threshold on the proposal — not on
    /// the consuming surface — pins the policy to the heuristic
    /// author's intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_accept_threshold: Option<ConfidenceBps>,
    /// Structured evidence the operator inspects. Wire-stable JSON
    /// so the same payload renders in both the FE and the audit
    /// row.
    pub evidence: serde_json::Value,
    /// Stable evidence digest used to dedup proposals across runs.
    /// Same `(detector, version, evidence_hash)` triple = same
    /// proposal — repeated runs under unchanged data do not
    /// produce duplicate queue entries.
    pub evidence_hash: String,
    /// Lifecycle state. `Pending` while the proposal sits in the
    /// queue; transitions through [`HeuristicProvenance::record`].
    pub state: HeuristicProposalState,
    /// Inclusive lower bound on the proposal's review window.
    /// `now()` at creation. Pinned so a cleanup cron can compute
    /// staleness deterministically.
    pub created_at: DateTime<Utc>,
    /// Exclusive upper bound. Past this instant, the cleanup cron
    /// transitions the proposal to `Expired`. Detector authors
    /// pick the window — a per-introspection fingerprint detector
    /// might pick 30 days, an LLM-driven proposal a few hours.
    pub expires_at: DateTime<Utc>,
}

impl HeuristicProposal {
    /// Construct a proposal with a deterministic `id` derived from
    /// the `(detector, version, evidence_hash)` triple. Re-running
    /// a detector against unchanged data therefore produces the
    /// same id, which the queue dedups via primary key.
    pub fn new(
        detector: HeuristicDetectorKind,
        detector_version: u32,
        confidence: ConfidenceBps,
        evidence: serde_json::Value,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        let evidence_hash = hash_evidence(&detector, detector_version, &evidence);
        let id = HeuristicProposalId::new(format!("hp-{evidence_hash}"));
        Self {
            id,
            detector,
            detector_version,
            confidence,
            auto_accept_threshold: None,
            evidence,
            evidence_hash,
            state: HeuristicProposalState::Pending,
            created_at,
            expires_at,
        }
    }

    /// Whether the consuming surface may auto-accept without
    /// operator review. Returns `false` when no threshold is set
    /// or when the proposal's confidence is strictly below the
    /// threshold — equality is sufficient (the author's intent
    /// reads naturally as "≥ threshold").
    pub fn meets_auto_accept_threshold(&self) -> bool {
        match self.auto_accept_threshold {
            Some(threshold) => self.confidence >= threshold,
            None => false,
        }
    }
}

/// Audit record for one proposal lifecycle event.
///
/// `decision` mirrors the proposal's terminal state. `decided_by`
/// is the principal id (user uuid / agent identifier); the audit
/// uses `String` to accept either shape. `evidence_hash` cross-
/// references the original proposal so a future review can confirm
/// the decision was made against the same payload (a heuristic
/// re-run with different evidence produces a new proposal with a
/// new hash, leaving this audit row pinned to the original).
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct HeuristicProvenance {
    pub id: HeuristicProvenanceId,
    pub proposal_id: HeuristicProposalId,
    pub detector: HeuristicDetectorKind,
    pub detector_version: u32,
    pub decision: HeuristicProposalState,
    pub decided_by: String,
    pub decided_at: DateTime<Utc>,
    pub evidence_hash: String,
    /// Operator-supplied reason. Optional on accept (a clean
    /// confidence-meets-threshold accept needs no narrative);
    /// strongly encouraged on decline so the next detector run
    /// can incorporate the rejection rationale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl HeuristicProvenance {
    /// Stamp a decision against a proposal. Mints a fresh
    /// provenance id and records the decision metadata. Callers
    /// must persist the row alongside the updated proposal state.
    pub fn record(
        proposal: &HeuristicProposal,
        decision: HeuristicProposalState,
        decided_by: impl Into<String>,
        decided_at: DateTime<Utc>,
        reason: Option<String>,
    ) -> Self {
        Self {
            id: HeuristicProvenanceId::new(format!("hpv-{}", Uuid::new_v4())),
            proposal_id: proposal.id.clone(),
            detector: proposal.detector.clone(),
            detector_version: proposal.detector_version,
            decision,
            decided_by: decided_by.into(),
            decided_at,
            evidence_hash: proposal.evidence_hash.clone(),
            reason,
        }
    }
}

/// SHA-256 over the canonical (detector-id, version, evidence)
/// triple. Detector id + version live alongside the evidence so
/// two detectors producing identical evidence still hash to
/// distinct values.
fn hash_evidence(
    detector: &HeuristicDetectorKind,
    detector_version: u32,
    evidence: &serde_json::Value,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(detector.id().as_bytes());
    hasher.update(detector_version.to_le_bytes());
    // `to_string` is sufficient for the dedup contract — a value
    // that re-serialises to a different string indicates evidence
    // drift, which is exactly what should produce a new hash.
    hasher.update(evidence.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn confidence_from_bps_caps_at_full() {
        assert_eq!(ConfidenceBps::from_bps(11_000).as_bps(), 10_000);
        assert_eq!(ConfidenceBps::from_bps(0).as_bps(), 0);
        assert_eq!(ConfidenceBps::from_bps(7_500).as_bps(), 7_500);
    }

    #[test]
    fn confidence_from_ratio_handles_nan_and_out_of_range() {
        assert_eq!(ConfidenceBps::from_ratio(f64::NAN).as_bps(), 0);
        assert_eq!(ConfidenceBps::from_ratio(-0.5).as_bps(), 0);
        assert_eq!(ConfidenceBps::from_ratio(2.0).as_bps(), 10_000);
        assert_eq!(ConfidenceBps::from_ratio(0.5).as_bps(), 5_000);
    }

    #[test]
    fn confidence_round_trips_to_ratio() {
        let c = ConfidenceBps::from_bps(7_500);
        assert!((c.as_ratio() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn proposal_id_is_deterministic_per_evidence() {
        let evidence = serde_json::json!({"col": "id", "matches": ["pk", "users.id"]});
        let now = now();
        let exp = now + Duration::hours(24);
        let p1 = HeuristicProposal::new(
            HeuristicDetectorKind::ForeignKeyInference,
            1,
            ConfidenceBps::from_bps(8_500),
            evidence.clone(),
            now,
            exp,
        );
        let p2 = HeuristicProposal::new(
            HeuristicDetectorKind::ForeignKeyInference,
            1,
            ConfidenceBps::from_bps(8_500),
            evidence,
            now,
            exp,
        );
        assert_eq!(p1.id, p2.id, "same evidence must dedup to same id");
        assert_eq!(p1.evidence_hash, p2.evidence_hash);
    }

    #[test]
    fn proposal_id_changes_when_evidence_changes() {
        let now = now();
        let exp = now + Duration::hours(24);
        let p1 = HeuristicProposal::new(
            HeuristicDetectorKind::ForeignKeyInference,
            1,
            ConfidenceBps::from_bps(8_500),
            serde_json::json!({"col": "id"}),
            now,
            exp,
        );
        let p2 = HeuristicProposal::new(
            HeuristicDetectorKind::ForeignKeyInference,
            1,
            ConfidenceBps::from_bps(8_500),
            serde_json::json!({"col": "user_id"}),
            now,
            exp,
        );
        assert_ne!(p1.id, p2.id);
        assert_ne!(p1.evidence_hash, p2.evidence_hash);
    }

    #[test]
    fn proposal_id_changes_when_detector_version_bumps() {
        let now = now();
        let exp = now + Duration::hours(24);
        let evidence = serde_json::json!({"col": "id"});
        let v1 = HeuristicProposal::new(
            HeuristicDetectorKind::ForeignKeyInference,
            1,
            ConfidenceBps::from_bps(8_500),
            evidence.clone(),
            now,
            exp,
        );
        let v2 = HeuristicProposal::new(
            HeuristicDetectorKind::ForeignKeyInference,
            2,
            ConfidenceBps::from_bps(8_500),
            evidence,
            now,
            exp,
        );
        assert_ne!(
            v1.id, v2.id,
            "version bump must produce a new proposal — auto-accept policy is keyed on (detector, version)"
        );
    }

    #[test]
    fn auto_accept_threshold_is_inclusive_at_equality() {
        let now = now();
        let exp = now + Duration::hours(24);
        let mut p = HeuristicProposal::new(
            HeuristicDetectorKind::GlossaryBindingSuggestion,
            1,
            ConfidenceBps::from_bps(8_000),
            serde_json::json!({}),
            now,
            exp,
        );
        p.auto_accept_threshold = Some(ConfidenceBps::from_bps(8_000));
        assert!(p.meets_auto_accept_threshold());
    }

    #[test]
    fn auto_accept_rejected_below_threshold() {
        let now = now();
        let exp = now + Duration::hours(24);
        let mut p = HeuristicProposal::new(
            HeuristicDetectorKind::GlossaryBindingSuggestion,
            1,
            ConfidenceBps::from_bps(6_999),
            serde_json::json!({}),
            now,
            exp,
        );
        p.auto_accept_threshold = Some(ConfidenceBps::from_bps(7_000));
        assert!(!p.meets_auto_accept_threshold());
    }

    #[test]
    fn auto_accept_requires_explicit_threshold() {
        let now = now();
        let exp = now + Duration::hours(24);
        let p = HeuristicProposal::new(
            HeuristicDetectorKind::GlossaryBindingSuggestion,
            1,
            ConfidenceBps::from_bps(9_999),
            serde_json::json!({}),
            now,
            exp,
        );
        assert!(
            !p.meets_auto_accept_threshold(),
            "even maximum confidence must require operator review when no threshold is set"
        );
    }

    #[test]
    fn provenance_pins_decision_and_carries_evidence_hash() {
        let now = now();
        let exp = now + Duration::hours(24);
        let proposal = HeuristicProposal::new(
            HeuristicDetectorKind::PiiHeuristicScan,
            1,
            ConfidenceBps::from_bps(8_500),
            serde_json::json!({"column": "ssn", "match": "regex"}),
            now,
            exp,
        );
        let prov = HeuristicProvenance::record(
            &proposal,
            HeuristicProposalState::Accepted,
            "alice",
            now + Duration::minutes(5),
            Some("matches policy".to_string()),
        );
        assert_eq!(prov.proposal_id, proposal.id);
        assert_eq!(prov.evidence_hash, proposal.evidence_hash);
        assert_eq!(prov.decision, HeuristicProposalState::Accepted);
        assert_eq!(prov.decided_by, "alice");
    }

    #[test]
    fn detector_kind_round_trips_through_serde() {
        let cases = vec![
            HeuristicDetectorKind::ForeignKeyInference,
            HeuristicDetectorKind::ValueSetInference,
            HeuristicDetectorKind::NotationPatternInference,
            HeuristicDetectorKind::GlossaryBindingSuggestion,
            HeuristicDetectorKind::ReconcileFuzzyMatch,
            HeuristicDetectorKind::PiiHeuristicScan,
            HeuristicDetectorKind::FederatedEdgeProposal,
            HeuristicDetectorKind::Custom {
                detector_id: "custom_xyz".to_string(),
            },
        ];
        for c in cases {
            let json = serde_json::to_string(&c).expect("serialise");
            let back: HeuristicDetectorKind = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(back, c);
        }
    }
}

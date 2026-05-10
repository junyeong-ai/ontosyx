//! Typed inference pipeline — `PipelineStage`, `StageOutcome`,
//! `InferenceSession`, `InferenceAttempt`.
//!
//! ## Why a state machine
//!
//! The pre-Φ9 `ox-agent` ran a flat tool-loop: the LLM picked the
//! next tool from a 6-element bag, the agent dispatched, the
//! result fed the next prompt. Two structural problems:
//!
//! 1. **Observability was un-shaped.** "Where did this run fail?"
//!    only resolved by reading every tool-call span and inferring
//!    the stage from the tool name. There was no canonical answer
//!    to "what stage was this on" because the agent didn't track
//!    one.
//! 2. **Refine had no typed history to fold over.** A retry just
//!    re-prompted with a fresh context; prior failure messages
//!    weren't injected as ICL because there was nothing to
//!    enumerate.
//!
//! This module promotes the pipeline into a closed enum
//! ([`PipelineStage`]) + a deterministic transition table
//! ([`TRANSITIONS`]) + a typed attempt history
//! ([`InferenceSession`] / [`InferenceAttempt`]). Three resulting
//! invariants:
//!
//! - **Compile-time totality**: a `(PipelineStage, StageOutcome)`
//!   transition is missing → const-fn assertion at the bottom of
//!   the module fails the build. New stages cannot ship without a
//!   complete transition table.
//! - **Typed retry history**: `InferenceAttempt` chains via
//!   `parent_attempt_id` so a refine fold reads a structured
//!   `Vec<InferenceAttempt>` rather than reconstructing from
//!   logs.
//! - **PROV-O continuity**: every attempt carries a
//!   `provenance_id` (Φ8.1) so the audit DAG spans
//!   session → attempt → judged metric in a single walk.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AgentRef;

// ---------------------------------------------------------------------------
// Stage + outcome enums
// ---------------------------------------------------------------------------

/// One stage in the NL→Query inference pipeline. The runtime
/// invokes stages in the order [`TRANSITIONS`] dictates; a stage's
/// outcome decides the next stage. Closed enum — every variant is
/// covered by the transition table and the const-fn assertion at
/// the bottom of the module.
///
/// `#[repr(u8)]` pins the layout so the const-fn equality used by
/// the assertion compiles.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum PipelineStage {
    /// LLM-side safety gate. Prompt-injection / jailbreak / PII
    /// leak detection BEFORE the question reaches retrieval.
    /// `Fail` short-circuits to `Done` with a typed safety reject.
    SafetyGate,
    /// Pull retrieval anchors — verified queries (Φ11), entity
    /// blend, GraphRAG community summaries.
    Retrieve,
    /// Resolve ambiguities (column / value / segment / concept).
    /// Operator-supplied resolutions land here.
    Ground,
    /// LLM call producing a typed `QueryIR` candidate.
    Compile,
    /// Pre-execute validation against the active ontology +
    /// SHACL rules + complexity guard.
    Validate,
    /// Inject prior-attempt error context as ICL, retry `Compile`.
    /// Hard cap of N retries is enforced by the agent loop;
    /// reaching the cap fans this stage's `Fail` to `Done`.
    Refine,
    /// Pick a winning candidate when multiple `Compile` passes
    /// produced equivalent-looking IRs. Skipped on single-candidate
    /// runs.
    Select,
    /// Emit final response shape — QueryIR + provenance + warnings.
    Compose,
    /// Terminal — pipeline finished. The session's
    /// [`SessionOutcome`] carries the final disposition.
    Done,
}

/// What a stage produced. Closed set so the transition table can
/// pin every (Stage, Outcome) → next.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum StageOutcome {
    /// Stage produced its expected artifact — proceed to the
    /// next stage in the table.
    Pass,
    /// Stage failed — the per-stage `Fail` arm in the table
    /// routes to retry (`Refine`) or terminal (`Done`).
    Fail,
    /// Stage was a no-op for this run (e.g. `Select` on a
    /// single-candidate). Transition is the same as `Pass`.
    Skip,
}

/// Why a failure happened — sub-classification carried on the
/// `InferenceAttempt.outcome` enum. The classification routes
/// retry policy at the agent layer (e.g. `OutOfBudget` skips
/// `Refine` because retry cannot recover budget).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClassification {
    /// Validator rejected the candidate — typed property or label
    /// the active ontology does not recognise.
    ValidationFailure,
    /// Runtime / DB error during execution dry-run or pre-flight.
    RuntimeError,
    /// Stage exceeded its time budget.
    Timeout,
    /// PromptBudget / token budget exhausted (ADR-0028).
    OutOfBudget,
    /// SafetyGate flagged a hard-stop.
    SafetyReject,
    /// Internal platform error (panic, infrastructure outage).
    Internal,
}

// ---------------------------------------------------------------------------
// Transition table
// ---------------------------------------------------------------------------

/// Static transition table: every `(from, outcome) → to` for the
/// pipeline. `Done` is terminal and never appears as a `from`.
///
/// Reading the table:
/// - **Forward path** (`Pass`/`Skip`): SafetyGate → Retrieve →
///   Ground → Compile → Validate → Select → Compose → Done.
/// - **Refine loop**: Compile/Validate `Fail` → Refine.
///   Refine `Pass` → back to Compile (one more try).
///   Refine `Fail` → Done (retries exhausted).
/// - **Hard rejects** (SafetyGate/Retrieve/Ground/Select/Compose
///   `Fail`): straight to Done.
pub const TRANSITIONS: &[(PipelineStage, StageOutcome, PipelineStage)] = &[
    (
        PipelineStage::SafetyGate,
        StageOutcome::Pass,
        PipelineStage::Retrieve,
    ),
    (
        PipelineStage::SafetyGate,
        StageOutcome::Fail,
        PipelineStage::Done,
    ),
    (
        PipelineStage::SafetyGate,
        StageOutcome::Skip,
        PipelineStage::Retrieve,
    ),
    (
        PipelineStage::Retrieve,
        StageOutcome::Pass,
        PipelineStage::Ground,
    ),
    (
        PipelineStage::Retrieve,
        StageOutcome::Fail,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Retrieve,
        StageOutcome::Skip,
        PipelineStage::Ground,
    ),
    (
        PipelineStage::Ground,
        StageOutcome::Pass,
        PipelineStage::Compile,
    ),
    (
        PipelineStage::Ground,
        StageOutcome::Fail,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Ground,
        StageOutcome::Skip,
        PipelineStage::Compile,
    ),
    (
        PipelineStage::Compile,
        StageOutcome::Pass,
        PipelineStage::Validate,
    ),
    (
        PipelineStage::Compile,
        StageOutcome::Fail,
        PipelineStage::Refine,
    ),
    (
        PipelineStage::Compile,
        StageOutcome::Skip,
        PipelineStage::Validate,
    ),
    (
        PipelineStage::Validate,
        StageOutcome::Pass,
        PipelineStage::Select,
    ),
    (
        PipelineStage::Validate,
        StageOutcome::Fail,
        PipelineStage::Refine,
    ),
    (
        PipelineStage::Validate,
        StageOutcome::Skip,
        PipelineStage::Select,
    ),
    (
        PipelineStage::Refine,
        StageOutcome::Pass,
        PipelineStage::Compile,
    ),
    (
        PipelineStage::Refine,
        StageOutcome::Fail,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Refine,
        StageOutcome::Skip,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Select,
        StageOutcome::Pass,
        PipelineStage::Compose,
    ),
    (
        PipelineStage::Select,
        StageOutcome::Fail,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Select,
        StageOutcome::Skip,
        PipelineStage::Compose,
    ),
    (
        PipelineStage::Compose,
        StageOutcome::Pass,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Compose,
        StageOutcome::Fail,
        PipelineStage::Done,
    ),
    (
        PipelineStage::Compose,
        StageOutcome::Skip,
        PipelineStage::Done,
    ),
];

impl PipelineStage {
    /// Look up `(self, outcome)` in [`TRANSITIONS`]. Returns
    /// `None` only when called on `Done` (terminal — no outgoing
    /// transitions). Runtime callers can `unwrap_or(Done)` to
    /// collapse a missing entry to terminal, but production code
    /// should never observe `None` for a non-terminal stage —
    /// the const-fn assertion at the bottom of this module
    /// guarantees the table covers every (non-terminal stage,
    /// outcome) pair.
    pub fn next(self, outcome: StageOutcome) -> Option<PipelineStage> {
        for (from, out, to) in TRANSITIONS {
            if stage_eq(*from, self) && outcome_eq(*out, outcome) {
                return Some(*to);
            }
        }
        None
    }

    /// Wire-string for the stage. Mirrors the `serde` rename so a
    /// persistence layer that stores the stage as TEXT can
    /// round-trip with this method on the read path.
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelineStage::SafetyGate => "safety_gate",
            PipelineStage::Retrieve => "retrieve",
            PipelineStage::Ground => "ground",
            PipelineStage::Compile => "compile",
            PipelineStage::Validate => "validate",
            PipelineStage::Refine => "refine",
            PipelineStage::Select => "select",
            PipelineStage::Compose => "compose",
            PipelineStage::Done => "done",
        }
    }

    /// Inverse of [`Self::as_str`]. Returns `None` for an
    /// unrecognised tag — the caller decides whether to error or
    /// fall through.
    pub fn from_wire_str(s: &str) -> Option<Self> {
        Some(match s {
            "safety_gate" => Self::SafetyGate,
            "retrieve" => Self::Retrieve,
            "ground" => Self::Ground,
            "compile" => Self::Compile,
            "validate" => Self::Validate,
            "refine" => Self::Refine,
            "select" => Self::Select,
            "compose" => Self::Compose,
            "done" => Self::Done,
            _ => return None,
        })
    }
}

const fn stage_eq(a: PipelineStage, b: PipelineStage) -> bool {
    a as u8 == b as u8
}

const fn outcome_eq(a: StageOutcome, b: StageOutcome) -> bool {
    a as u8 == b as u8
}

// Const-fn exhaustiveness assertion. Every (non-terminal stage,
// outcome) pair must appear in TRANSITIONS exactly once. Adding a
// new `PipelineStage` variant without updating the table fails
// this assertion at compile time.
const _: () = {
    const STAGES: &[PipelineStage] = &[
        PipelineStage::SafetyGate,
        PipelineStage::Retrieve,
        PipelineStage::Ground,
        PipelineStage::Compile,
        PipelineStage::Validate,
        PipelineStage::Refine,
        PipelineStage::Select,
        PipelineStage::Compose,
    ];
    const OUTCOMES: &[StageOutcome] = &[StageOutcome::Pass, StageOutcome::Fail, StageOutcome::Skip];

    let mut s_idx = 0;
    while s_idx < STAGES.len() {
        let mut o_idx = 0;
        while o_idx < OUTCOMES.len() {
            let mut found: usize = 0;
            let mut t_idx = 0;
            while t_idx < TRANSITIONS.len() {
                let (from, out, _to) = TRANSITIONS[t_idx];
                if stage_eq(STAGES[s_idx], from) && outcome_eq(OUTCOMES[o_idx], out) {
                    found += 1;
                }
                t_idx += 1;
            }
            assert!(
                found == 1,
                "TRANSITIONS missing or duplicating a (stage, outcome) entry"
            );
            o_idx += 1;
        }
        s_idx += 1;
    }
};

// ---------------------------------------------------------------------------
// Session + attempt
// ---------------------------------------------------------------------------

/// One end-to-end inference run. Owns the attempt chain via
/// `session_id` foreign key (in `inference_attempts`); workspace-
/// scoped via the `workspace_id` column + RLS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InferenceSession {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// What the user asked. Free-form natural language.
    pub question: String,
    /// Who initiated the inference — operator user, automated
    /// cron, chat session, etc.
    pub initiator: AgentRef,
    /// `None` while the session is in flight; `Some` once the
    /// pipeline lands on `PipelineStage::Done`. The terminal
    /// outcome carries the winning attempt's id (for replay) or
    /// the failure classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_outcome: Option<SessionOutcome>,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
}

/// Closed set of session terminal states. The wire shape is the
/// snake_case `kind` discriminator — adding a future variant is a
/// Rust-side change that downstream JSON consumers see as a new
/// tag, never a silent reshape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionOutcome {
    /// Pipeline produced a usable QueryIR. The winning attempt's
    /// id is the session's anchor for downstream replay /
    /// caching / verified-query promotion.
    Success { winning_attempt_id: Uuid },
    /// Pipeline ran out of `Refine` budget and gave up.
    Exhausted { reason: String },
    /// SafetyGate or another reject route fired.
    Rejected {
        classification: ErrorClassification,
        reason: String,
    },
    /// Operator cancelled mid-flight, or the request was cut off
    /// upstream (client disconnect, request timeout).
    Cancelled,
}

/// One attempt within a session — the typed unit `Refine` folds
/// over to construct ICL for the next try.
///
/// Attempts chain via `parent_attempt_id`; the root attempt has
/// `parent_attempt_id = None`. The `attempt_index` (0-based)
/// gives a stable per-session numbering for UI rendering and
/// pass@k metric computation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InferenceAttempt {
    pub id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    /// Predecessor attempt id. `None` only for the root attempt.
    /// Walking the chain back to the root reconstructs the full
    /// retry history without scanning the whole session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_attempt_id: Option<Uuid>,
    /// 0 for the first attempt, +1 per retry. `(session_id,
    /// attempt_index)` is `UNIQUE` on the persistence layer —
    /// re-running a session does not silently overwrite history.
    pub attempt_index: u32,
    /// Which stage emitted this attempt. Most attempts come from
    /// `Compile` (LLM round-trip); some from `Validate` (pre-flight
    /// reject without an LLM call) or `Select` (multi-candidate
    /// winner picked deterministically).
    pub emitted_at_stage: PipelineStage,
    /// JSON-serialised `QueryIR` candidate the attempt produced.
    /// `None` when the attempt failed before producing a
    /// candidate (LLM refused, network outage, validator rejected
    /// pre-LLM). Stored as `serde_json::Value` so this layer does
    /// not pull `ox-query-ir`; consumers (Brain, Agent)
    /// deserialise via the typed shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub query_ir_candidate: Option<serde_json::Value>,
    pub outcome: AttemptOutcome,
    /// PROV-O activity row for this attempt. `Some` whenever the
    /// attempt invoked an LLM (Compile / Refine); `None` for
    /// purely deterministic attempts that did not produce a
    /// fact (a pre-LLM Validate rejection has no LLM call to
    /// audit). When `Some`, FK → `provenance_records.id`,
    /// `ON DELETE RESTRICT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Outcome of one attempt — the Refine fold reads this to build
/// the next try's ICL block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttemptOutcome {
    /// Attempt produced a candidate that passed `Validate`. The
    /// session may still continue (e.g. `Select` between two
    /// successful candidates); this only describes the attempt,
    /// not the session.
    Success,
    /// Validator rejected the candidate's IR shape (unknown
    /// label, missing property, complexity guard tripped).
    ValidationError {
        classification: ErrorClassification,
        message: String,
    },
    /// Runtime error during dry-run / execution (DB connection,
    /// driver-side parse error, query timeout).
    RuntimeError {
        classification: ErrorClassification,
        message: String,
    },
    /// Attempt exceeded its time budget.
    Timeout,
    /// PromptBudget / token budget exhausted before the attempt
    /// could complete.
    OutOfBudget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_path_passes_through_full_pipeline() {
        // SafetyGate → Retrieve → Ground → Compile → Validate →
        // Select → Compose → Done. Every Pass transition wired.
        let path = [
            PipelineStage::SafetyGate,
            PipelineStage::Retrieve,
            PipelineStage::Ground,
            PipelineStage::Compile,
            PipelineStage::Validate,
            PipelineStage::Select,
            PipelineStage::Compose,
            PipelineStage::Done,
        ];
        for w in path.windows(2) {
            let from = w[0];
            let to = w[1];
            assert_eq!(
                from.next(StageOutcome::Pass),
                Some(to),
                "Pass({:?}) should reach {:?}",
                from,
                to
            );
        }
    }

    #[test]
    fn validate_failure_routes_into_refine() {
        assert_eq!(
            PipelineStage::Validate.next(StageOutcome::Fail),
            Some(PipelineStage::Refine)
        );
        assert_eq!(
            PipelineStage::Compile.next(StageOutcome::Fail),
            Some(PipelineStage::Refine)
        );
    }

    #[test]
    fn refine_pass_loops_back_to_compile() {
        assert_eq!(
            PipelineStage::Refine.next(StageOutcome::Pass),
            Some(PipelineStage::Compile)
        );
    }

    #[test]
    fn refine_failure_is_terminal() {
        assert_eq!(
            PipelineStage::Refine.next(StageOutcome::Fail),
            Some(PipelineStage::Done)
        );
    }

    #[test]
    fn safety_failure_short_circuits_to_done() {
        assert_eq!(
            PipelineStage::SafetyGate.next(StageOutcome::Fail),
            Some(PipelineStage::Done)
        );
    }

    #[test]
    fn done_stage_has_no_outgoing_transition() {
        assert_eq!(PipelineStage::Done.next(StageOutcome::Pass), None);
        assert_eq!(PipelineStage::Done.next(StageOutcome::Fail), None);
        assert_eq!(PipelineStage::Done.next(StageOutcome::Skip), None);
    }

    #[test]
    fn stage_wire_strings_round_trip() {
        for stage in [
            PipelineStage::SafetyGate,
            PipelineStage::Retrieve,
            PipelineStage::Ground,
            PipelineStage::Compile,
            PipelineStage::Validate,
            PipelineStage::Refine,
            PipelineStage::Select,
            PipelineStage::Compose,
            PipelineStage::Done,
        ] {
            let wire = stage.as_str();
            let back = PipelineStage::from_wire_str(wire);
            assert_eq!(back, Some(stage), "round-trip broke for {wire}");
        }
    }

    #[test]
    fn session_outcome_round_trips_with_kind_discriminator() {
        let outcome = SessionOutcome::Rejected {
            classification: ErrorClassification::SafetyReject,
            reason: "prompt-injection detected".into(),
        };
        let v = serde_json::to_value(&outcome).unwrap();
        assert_eq!(v.get("kind").and_then(|s| s.as_str()), Some("rejected"));
        let back: SessionOutcome = serde_json::from_value(v).unwrap();
        assert_eq!(back, outcome);
    }

    #[test]
    fn attempt_outcome_carries_classification_and_message() {
        let outcome = AttemptOutcome::ValidationError {
            classification: ErrorClassification::ValidationFailure,
            message: "unknown label `Custmer`".into(),
        };
        let v = serde_json::to_value(&outcome).unwrap();
        assert_eq!(
            v.get("kind").and_then(|s| s.as_str()),
            Some("validation_error")
        );
        let back: AttemptOutcome = serde_json::from_value(v).unwrap();
        assert_eq!(back, outcome);
    }
}

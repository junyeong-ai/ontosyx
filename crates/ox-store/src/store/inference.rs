//! Inference-pipeline persistence — sessions + attempt chains.
//!
//! Backs the `inference_sessions` + `inference_attempts` tables.
//! The pipeline state machine
//! (`ox_ontology::PipelineStage` + `TRANSITIONS`) is closed at
//! the type level; this trait persists the per-session attempt
//! chain so `Refine` can fold over prior failures as ICL and the
//! audit/diagnostics surface can render "where did this run
//! fail" without log archaeology.
//!
//! ## Method shape
//!
//! Every method takes the workspace via the bound `WORKSPACE_ID`
//! task-local. RLS scopes reads + writes; cross-tenant ids
//! resolve to `None` on lookup.
//!
//! Append-only writes — sessions get one `create_session` and
//! one `complete_session`; attempts get one `record_attempt` per
//! pipeline iteration. There is no "edit attempt in place" path:
//! re-running a session creates a fresh session id; re-trying
//! within a session creates a fresh attempt with a new index.
//!
//! ## Provenance integration
//!
//! `record_attempt` accepts an `Option<ProvenanceCapture>` —
//! `Some` for LLM-driven attempts (Compile / Refine), `None` for
//! purely deterministic attempts (pre-LLM Validate reject). When
//! `Some`, the impl stamps a row through `ProvenanceStore`
//! before inserting the attempt so the FK stays coherent.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;
use ox_ontology::{
    AgentRef, AttemptOutcome, InferenceAttempt, InferenceSession, PipelineStage, ProvenanceCapture,
    SessionOutcome,
};

#[async_trait]
pub trait InferenceSessionStore: Send + Sync {
    /// Open a fresh session. The pipeline lands attempts under
    /// the returned `id`. Status: in-flight (`final_outcome =
    /// None`, `ended_at = None`).
    async fn create_inference_session(
        &self,
        question: &str,
        initiator: AgentRef,
    ) -> OxResult<InferenceSession>;

    /// Fetch a session by id — RLS-scoped, returns `None` for
    /// cross-tenant ids.
    async fn get_inference_session(&self, id: Uuid) -> OxResult<Option<InferenceSession>>;

    /// Append one attempt to `session_id`. The next free index is
    /// computed inside the call (single round-trip), so concurrent
    /// callers never collide on `(session_id, attempt_index)` —
    /// the impl uses `INSERT ... SELECT next_index ... ON CONFLICT
    /// DO NOTHING` and retries once on conflict.
    ///
    /// `capture` stamps a `provenance_records` row whose id lands
    /// on the attempt's `provenance_id` FK. Pass `None` when the
    /// attempt is purely deterministic and never invoked an LLM.
    async fn record_inference_attempt(
        &self,
        session_id: Uuid,
        parent_attempt_id: Option<Uuid>,
        emitted_at_stage: PipelineStage,
        query_ir_candidate: Option<serde_json::Value>,
        outcome: AttemptOutcome,
        capture: Option<ProvenanceCapture>,
    ) -> OxResult<InferenceAttempt>;

    /// List every attempt under `session_id`, ordered by
    /// `attempt_index` ASC so a `Refine` fold reads them in
    /// chronological order.
    async fn list_inference_attempts(&self, session_id: Uuid) -> OxResult<Vec<InferenceAttempt>>;

    /// Transition a session to its terminal state — sets
    /// `final_outcome` + stamps `ended_at = now()`. Domain verb
    /// because the audit semantics live here: downstream
    /// dashboards key on `ended_at` to dim in-flight rows.
    async fn complete_inference_session(
        &self,
        id: Uuid,
        outcome: SessionOutcome,
    ) -> OxResult<InferenceSession>;
}

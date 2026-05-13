//! [`InferenceContext`] task-local + session finalisation helpers.
//!
//! Mirrors the `EvaluationContext` shape: an outer caller (the
//! agent's `query_graph` tool, the MCP server, the evaluation
//! case-execute path) opens an `InferenceSession` via
//! [`crate::store::InferenceSessionStore::create_inference_session`],
//! binds the resulting id into [`InferenceContext`], and runs the
//! Brain-side translate flow inside `scope_inference_context`.
//! Inner layers (Brain's `translate_query`) read the active
//! context via [`current_inference_context`] and call
//! `record_inference_attempt` for each LLM iteration without
//! threading the session id through every signature.
//!
//! ## Why a task-local
//!
//! Threading the session id through every call site
//! (`translate_query` → `call_structured_traced` → tool dispatch)
//! pollutes every inner signature with audit infrastructure that
//! 99% of internal callers don't need to know about. The same
//! argument that justified `EvaluationContext` justifies this:
//! the audit shape lives at the seam between caller (the
//! pipeline's outer driver) and the Brain (the inner LLM
//! invoker), not in the middle layers.

use std::future::Future;

use uuid::Uuid;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{AttemptOutcome, ErrorClassification, SessionOutcome};

use crate::store::Store;

/// Per-task inference context. Bound by the pipeline's outer
/// driver via [`scope_inference_context`]; read by the Brain's
/// `translate_query` / `call_structured_traced` to attribute
/// per-LLM-call attempts to the right session.
#[derive(Debug, Clone)]
pub struct InferenceContext {
    /// `inference_sessions.id` for the active session. Every
    /// `record_inference_attempt` call inside this scope keys
    /// off this id.
    pub session_id: Uuid,
}

tokio::task_local! {
    static INFERENCE_CONTEXT: InferenceContext;
}

/// Read the active context. `None` outside any inference scope —
/// production traffic that never opens a session pays no overhead
/// because the Brain's record-call branch short-circuits on the
/// `None` arm.
pub fn current_inference_context() -> Option<InferenceContext> {
    INFERENCE_CONTEXT.try_with(|c| c.clone()).ok()
}

/// Run `fut` inside an inference scope. Every task spawned inside
/// the future inherits the binding only when it goes through the
/// workspace `spawn_scoped` helpers — `tokio::spawn` directly
/// detaches the task-local. The same isolation contract
/// `WORKSPACE_ID` follows.
pub async fn scope_inference_context<F, T>(ctx: InferenceContext, fut: F) -> T
where
    F: Future<Output = T>,
{
    INFERENCE_CONTEXT.scope(ctx, fut).await
}

/// Convenience: open a session, bind the context, run `body`,
/// finalise the session based on the result. Returns whatever
/// `body` returned. The session is always closed — `Ok` paths
/// resolve the winning attempt id from
/// `list_inference_attempts`; `Err` paths classify the failure
/// and stamp `SessionOutcome::Rejected`.
///
/// Callers that need finer control over the final outcome
/// (Cancelled, Exhausted with custom reason) skip this helper and
/// drive the lifecycle manually.
pub async fn run_in_inference_session<F, Fut, T>(
    store: &dyn Store,
    question: &str,
    initiator: ox_ontology::AgentRef,
    body: F,
) -> OxResult<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = OxResult<T>>,
{
    let session = store.create_inference_session(question, initiator).await?;
    let session_id = session.id;
    let result = scope_inference_context(InferenceContext { session_id }, body()).await;

    match &result {
        Ok(_) => {
            // Resolve winning attempt id from the recorded chain.
            // The convention is "last successful attempt wins" —
            // the Brain's retry path only flags `Success` once the
            // final candidate passed validate.
            let attempts = store.list_inference_attempts(session_id).await?;
            let winning = attempts
                .iter()
                .rev()
                .find(|a| matches!(a.outcome, AttemptOutcome::Success));
            let outcome = match winning {
                Some(a) => SessionOutcome::Success {
                    winning_attempt_id: a.id,
                },
                None => SessionOutcome::Exhausted {
                    reason: "session ended Ok but no Success-tagged attempt was recorded \
                             — the Brain may not be wired to record_inference_attempt yet"
                        .into(),
                },
            };
            store
                .complete_inference_session(session_id, outcome)
                .await?;
        }
        Err(err) => {
            let classification = classify_error(err);
            store
                .complete_inference_session(
                    session_id,
                    SessionOutcome::Rejected {
                        classification,
                        reason: format!("{err:?}").chars().take(500).collect(),
                    },
                )
                .await?;
        }
    }

    result
}

/// Map an `OxError` shape to the closed
/// [`ErrorClassification`] enum. Used by the session-rejection
/// path so dashboard / regression-gate rollups bucket failures
/// consistently.
fn classify_error(err: &OxError) -> ErrorClassification {
    match err {
        OxError::Validation { .. }
        | OxError::Compilation { .. }
        | OxError::NotFound { .. }
        | OxError::Conflict { .. }
        | OxError::Parse { .. }
        | OxError::Ontology { .. }
        | OxError::UnsupportedOperation { .. } => ErrorClassification::ValidationFailure,
        OxError::Runtime { .. }
        | OxError::Contextual { .. }
        | OxError::Serialization(_)
        | OxError::Llm { .. }
        | OxError::MissingContext { .. } => ErrorClassification::RuntimeError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn context_is_visible_inside_scope() {
        assert!(current_inference_context().is_none());
        let ctx = InferenceContext {
            session_id: Uuid::new_v4(),
        };
        let observed =
            scope_inference_context(ctx.clone(), async { current_inference_context() }).await;
        assert_eq!(observed.map(|c| c.session_id), Some(ctx.session_id));
        assert!(current_inference_context().is_none());
    }

    #[test]
    fn validation_errors_classify_as_validation_failure() {
        let e = OxError::Validation {
            field: "labels".into(),
            message: "bad".into(),
        };
        assert!(matches!(
            classify_error(&e),
            ErrorClassification::ValidationFailure
        ));
    }

    #[test]
    fn runtime_errors_classify_as_runtime() {
        let e = OxError::Runtime {
            message: "db down".into(),
        };
        assert!(matches!(
            classify_error(&e),
            ErrorClassification::RuntimeError
        ));
    }
}

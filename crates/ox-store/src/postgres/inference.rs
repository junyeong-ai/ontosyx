//! [`InferenceSessionStore`] postgres impl.
//!
//! Workspace-scoped via the bound task-local; RLS gates every
//! read + write. Append-only — no in-place edit on attempts; a
//! re-run goes through a new session id.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{
    AgentRef, AttemptOutcome, EntityRef, InferenceAttempt, InferenceSession, PipelineStage,
    ProvenanceCapture, SessionOutcome,
};

use crate::store::{InferenceSessionStore, ProvenanceStore};

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct InferenceSessionRow {
    id: Uuid,
    workspace_id: Uuid,
    question: String,
    initiator: serde_json::Value,
    final_outcome: Option<serde_json::Value>,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl InferenceSessionRow {
    fn into_domain(self) -> OxResult<InferenceSession> {
        let initiator: AgentRef =
            serde_json::from_value(self.initiator).map_err(|e| OxError::Runtime {
                message: format!("decode inference_sessions.initiator failed: {e}"),
            })?;
        let final_outcome: Option<SessionOutcome> = match self.final_outcome {
            Some(v) => Some(serde_json::from_value(v).map_err(|e| OxError::Runtime {
                message: format!("decode inference_sessions.final_outcome failed: {e}"),
            })?),
            None => None,
        };
        Ok(InferenceSession {
            id: self.id,
            workspace_id: self.workspace_id,
            question: self.question,
            initiator,
            final_outcome,
            started_at: self.started_at,
            ended_at: self.ended_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct InferenceAttemptRow {
    id: Uuid,
    session_id: Uuid,
    workspace_id: Uuid,
    parent_attempt_id: Option<Uuid>,
    attempt_index: i32,
    emitted_at_stage: String,
    query_ir_candidate: Option<serde_json::Value>,
    outcome: serde_json::Value,
    provenance_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl InferenceAttemptRow {
    fn into_domain(self) -> OxResult<InferenceAttempt> {
        let stage = PipelineStage::from_wire_str(&self.emitted_at_stage).ok_or_else(|| {
            OxError::Runtime {
                message: format!(
                    "inference_attempts.emitted_at_stage `{}` not a known PipelineStage \
                     wire string",
                    self.emitted_at_stage
                ),
            }
        })?;
        let outcome: AttemptOutcome =
            serde_json::from_value(self.outcome).map_err(|e| OxError::Runtime {
                message: format!("decode inference_attempts.outcome failed: {e}"),
            })?;
        let attempt_index =
            u32::try_from(self.attempt_index.max(0)).map_err(|e| OxError::Runtime {
                message: format!("inference_attempts.attempt_index overflow: {e}"),
            })?;
        Ok(InferenceAttempt {
            id: self.id,
            session_id: self.session_id,
            workspace_id: self.workspace_id,
            parent_attempt_id: self.parent_attempt_id,
            attempt_index,
            emitted_at_stage: stage,
            query_ir_candidate: self.query_ir_candidate,
            outcome,
            provenance_id: self.provenance_id,
            created_at: self.created_at,
        })
    }
}

#[async_trait]
impl InferenceSessionStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(question_len = question.len()))]
    async fn create_inference_session(
        &self,
        question: &str,
        initiator: AgentRef,
    ) -> OxResult<InferenceSession> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let initiator_json = serde_json::to_value(&initiator).map_err(|e| OxError::Runtime {
            message: format!("encode initiator failed: {e}"),
        })?;
        let id = Uuid::now_v7();
        let row: InferenceSessionRow = sqlx::query_as(
            "INSERT INTO inference_sessions
                (id, workspace_id, question, initiator, final_outcome,
                 started_at, ended_at)
             VALUES ($1, $2, $3, $4, NULL, now(), NULL)
             RETURNING id, workspace_id, question, initiator, final_outcome,
                       started_at, ended_at",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(question)
        .bind(&initiator_json)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(session_id = %id))]
    async fn get_inference_session(&self, id: Uuid) -> OxResult<Option<InferenceSession>> {
        super::require_workspace_context()?;
        let row: Option<InferenceSessionRow> = sqlx::query_as(
            "SELECT id, workspace_id, question, initiator, final_outcome,
                    started_at, ended_at
             FROM inference_sessions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(InferenceSessionRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        session_id = %session_id,
        stage = emitted_at_stage.as_str(),
    ))]
    async fn record_inference_attempt(
        &self,
        session_id: Uuid,
        parent_attempt_id: Option<Uuid>,
        emitted_at_stage: PipelineStage,
        query_ir_candidate: Option<serde_json::Value>,
        outcome: AttemptOutcome,
        capture: Option<ProvenanceCapture>,
    ) -> OxResult<InferenceAttempt> {
        let workspace_id = super::bound_workspace_id_for_dml()?;

        // Stamp provenance first when supplied — the FK requires
        // the audit row before the attempt INSERT can succeed.
        // Subject names the session this attempt belongs to so an
        // audit-trail walk lands at the originating session in one
        // hop.
        let attempt_id = Uuid::now_v7();
        let provenance_id = match capture {
            Some(cap) => {
                let subject = EntityRef::Arbitrary {
                    label: format!("inference_attempt:{attempt_id}"),
                };
                let prov_id_str = <Self as ProvenanceStore>::record_activity(self, cap, subject)
                    .await?
                    .as_str()
                    .to_string();
                Some(Uuid::parse_str(&prov_id_str).map_err(|e| OxError::Runtime {
                    message: format!(
                        "ProvenanceStore::record_activity returned non-UUID id \
                             `{prov_id_str}`: {e}"
                    ),
                })?)
            }
            None => None,
        };

        let outcome_json = serde_json::to_value(&outcome).map_err(|e| OxError::Runtime {
            message: format!("encode AttemptOutcome failed: {e}"),
        })?;

        // Compute the next attempt index in the same INSERT via
        // subselect. Concurrent callers race here only via
        // `(session_id, attempt_index)` UNIQUE — the loser
        // retries once with the bumped index. Single retry
        // covers contention; persistent collision indicates a
        // bug elsewhere and surfaces as the inner sqlx error.
        for retry in 0..2 {
            let row: Result<InferenceAttemptRow, sqlx::Error> = sqlx::query_as(
                "INSERT INTO inference_attempts
                    (id, session_id, workspace_id, parent_attempt_id,
                     attempt_index, emitted_at_stage, query_ir_candidate,
                     outcome, provenance_id, created_at)
                 SELECT $1, $2, $3, $4,
                        COALESCE((SELECT MAX(attempt_index) + 1
                                    FROM inference_attempts
                                   WHERE session_id = $2),
                                 0),
                        $5, $6, $7, $8, now()
                 RETURNING id, session_id, workspace_id, parent_attempt_id,
                           attempt_index, emitted_at_stage,
                           query_ir_candidate, outcome, provenance_id,
                           created_at",
            )
            .bind(attempt_id)
            .bind(session_id)
            .bind(workspace_id)
            .bind(parent_attempt_id)
            .bind(emitted_at_stage.as_str())
            .bind(query_ir_candidate.as_ref())
            .bind(&outcome_json)
            .bind(provenance_id)
            .fetch_one(&self.pool)
            .await;
            match row {
                Ok(r) => return r.into_domain(),
                Err(e) if retry == 0 && unique_violation(&e) => continue,
                Err(e) => return Err(to_ox_error(e)),
            }
        }
        Err(OxError::Conflict {
            message: format!(
                "inference_attempts unique-collision twice on session_id={session_id} \
                 — concurrent writers exceeded retry budget"
            ),
        })
    }

    #[tracing::instrument(level = "debug", skip_all, fields(session_id = %session_id))]
    async fn list_inference_attempts(&self, session_id: Uuid) -> OxResult<Vec<InferenceAttempt>> {
        super::require_workspace_context()?;
        let rows: Vec<InferenceAttemptRow> = sqlx::query_as(
            "SELECT id, session_id, workspace_id, parent_attempt_id,
                    attempt_index, emitted_at_stage, query_ir_candidate,
                    outcome, provenance_id, created_at
             FROM inference_attempts
             WHERE session_id = $1
             ORDER BY attempt_index ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(InferenceAttemptRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(session_id = %id))]
    async fn complete_inference_session(
        &self,
        id: Uuid,
        outcome: SessionOutcome,
    ) -> OxResult<InferenceSession> {
        super::require_workspace_context()?;
        let outcome_json = serde_json::to_value(&outcome).map_err(|e| OxError::Runtime {
            message: format!("encode SessionOutcome failed: {e}"),
        })?;
        let row: Option<InferenceSessionRow> = sqlx::query_as(
            "UPDATE inference_sessions
             SET final_outcome = $2, ended_at = now()
             WHERE id = $1
             RETURNING id, workspace_id, question, initiator, final_outcome,
                       started_at, ended_at",
        )
        .bind(id)
        .bind(&outcome_json)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.ok_or_else(|| OxError::NotFound {
            entity: format!("inference_sessions id={id}"),
        })?
        .into_domain()
    }
}

/// Postgres unique-violation classifier. Limited to the
/// `attempt_index` race in `record_inference_attempt`; surfaces
/// every other DB error as-is via `to_ox_error`.
fn unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db) = e
        && let Some(code) = db.code()
    {
        return code == "23505";
    }
    false
}

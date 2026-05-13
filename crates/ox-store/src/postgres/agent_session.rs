//! [`AgentSessionStore`] — LLM agent session + event log (turn-by-turn tool call trace).

use super::*;

#[derive(sqlx::FromRow)]
struct AgentSessionRow {
    id: Uuid,
    user_id: String,
    ontology_lineage_id: Option<String>,
    prompt_hash: String,
    tool_schema_hash: String,
    model_id: String,
    user_message: String,
    final_text: Option<String>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<AgentSessionRow> for AgentSession {
    fn from(row: AgentSessionRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            ontology_lineage_id: row.ontology_lineage_id,
            prompt_hash: row.prompt_hash,
            tool_schema_hash: row.tool_schema_hash,
            model_id: row.model_id,
            user_message: row.user_message,
            final_text: row.final_text,
            created_at: row.created_at,
            completed_at: row.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AgentEventRow {
    id: Uuid,
    session_id: Uuid,
    workspace_id: Uuid,
    sequence: i32,
    event_type: String,
    payload: sqlx::types::Json<AgentEventPayload>,
    created_at: DateTime<Utc>,
}

impl From<AgentEventRow> for AgentEvent {
    fn from(row: AgentEventRow) -> Self {
        Self {
            id: row.id,
            session_id: row.session_id,
            workspace_id: row.workspace_id,
            sequence: row.sequence,
            event_type: row.event_type,
            payload: row.payload.0,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl AgentSessionStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_agent_session(&self, s: &AgentSession) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO agent_sessions (id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
             model_id, user_message, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(s.id)
        .bind(&s.user_id)
        .bind(&s.ontology_lineage_id)
        .bind(&s.prompt_hash)
        .bind(&s.tool_schema_hash)
        .bind(&s.model_id)
        .bind(&s.user_message)
        .bind(s.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn complete_agent_session(&self, id: Uuid, final_text: Option<&str>) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE agent_sessions SET final_text = $2, completed_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(final_text)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_agent_session(&self, id: Uuid) -> OxResult<Option<AgentSession>> {
        let row: Option<AgentSessionRow> = sqlx::query_as(
            "SELECT id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
                    model_id, user_message, final_text,
                    created_at, completed_at
             FROM agent_sessions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(row.map(AgentSession::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_agent_sessions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AgentSession>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows: Vec<AgentSessionRow> = match &pagination.cursor {
            Some(cursor) => {
                let cursor_time: DateTime<Utc> =
                    cursor
                        .parse()
                        .map_err(|e: chrono::format::ParseError| OxError::Parse {
                            field: "cursor".into(),
                            source: Box::new(e),
                        })?;
                sqlx::query_as(
                    "SELECT id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
                            model_id, user_message, final_text,
                            created_at, completed_at
                     FROM agent_sessions WHERE user_id = $1 AND created_at < $2
                     ORDER BY created_at DESC LIMIT $3",
                )
                .bind(user_id)
                .bind(cursor_time)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Database error: {e}"),
                })?
            }
            None => sqlx::query_as(
                "SELECT id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
                            model_id, user_message, final_text,
                            created_at, completed_at
                     FROM agent_sessions WHERE user_id = $1
                     ORDER BY created_at DESC LIMIT $2",
            )
            .bind(user_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?,
        };

        let items: Vec<AgentSession> = rows.into_iter().map(AgentSession::from).collect();
        Ok(build_cursor_page(items, limit, |s| (s.created_at, s.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_agent_event(&self, e: &AgentEvent) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO agent_events (id, session_id, workspace_id, sequence, event_type, payload, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(e.id)
        .bind(e.session_id)
        .bind(e.workspace_id)
        .bind(e.sequence)
        .bind(&e.event_type)
        .bind(sqlx::types::Json(&e.payload))
        .bind(e.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_agent_events(&self, session_id: Uuid) -> OxResult<Vec<AgentEvent>> {
        let rows: Vec<AgentEventRow> = sqlx::query_as(
            "SELECT id, session_id, workspace_id, sequence, event_type, payload, created_at
             FROM agent_events WHERE session_id = $1 ORDER BY sequence",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(rows.into_iter().map(AgentEvent::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_agent_session(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        // Delete events first (explicit rather than relying on CASCADE)
        sqlx::query("DELETE FROM agent_events WHERE session_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;

        let result = sqlx::query("DELETE FROM agent_sessions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;

        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn cleanup_old_sessions(&self, retention_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        super::require_workspace_context()?;
        // Delete events first (CASCADE would handle this but be explicit)
        sqlx::query(
            "DELETE FROM agent_events WHERE session_id IN (
                SELECT id FROM agent_sessions WHERE created_at < NOW() - ($1 || ' days')::interval
            )",
        )
        .bind(retention_days)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;

        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 DELETE FROM agent_sessions
                 WHERE created_at < NOW() - ($1 || ' days')::interval
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .bind(retention_days)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;

        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }
}

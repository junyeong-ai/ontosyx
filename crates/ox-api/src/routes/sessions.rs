use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::{AgentEvent, AgentEventPayload, AgentSession, CursorParams, ToolApproval};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// GET /api/sessions — list agent sessions
//
// `CursorParams` lives in `ox-store`, which is utoipa-free; the
// route layer mirrors its shape via `SessionsCursorQuery` so the
// generated spec describes the wire surface without dragging
// utoipa into the persistence crate.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct SessionsCursorQuery {
    #[serde(default = "default_session_limit")]
    pub limit: u32,
    pub cursor: Option<String>,
}

fn default_session_limit() -> u32 {
    50
}

impl From<SessionsCursorQuery> for CursorParams {
    fn from(q: SessionsCursorQuery) -> Self {
        Self {
            limit: q.limit,
            cursor: q.cursor,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/sessions",
    params(SessionsCursorQuery),
    responses((status = 200, description = "Caller's agent sessions", body = crate::openapi::AgentSessionPage)),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    principal: Principal,
    Query(params): Query<SessionsCursorQuery>,
) -> Result<Json<ApiResponse<Vec<AgentSession>>>, AppError> {
    let params: CursorParams = params.into();
    let page = state
        .store
        .list_agent_sessions(&principal.id, &params)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// Shared: fetch session with ownership check
// ---------------------------------------------------------------------------

async fn load_owned_session(
    state: &AppState,
    principal: &Principal,
    id: Uuid,
) -> Result<AgentSession, AppError> {
    let session = state
        .store
        .get_agent_session(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Agent session"))?;

    if session.user_id != principal.id {
        return Err(AppError::resource_not_owned("session"));
    }

    Ok(session)
}

// ---------------------------------------------------------------------------
// GET /api/sessions/:id — get single session
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/sessions/{id}",
    params(("id" = Uuid, Path, description = "Agent session ID")),
    responses(
        (status = 200, description = "Agent session", body = AgentSession),
        (status = 404, description = "Session not found"),
    ),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn get_session(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<AgentSession>>, AppError> {
    let session = load_owned_session(&state, &principal, id).await?;
    Ok(ApiResponse::of(session))
}

// ---------------------------------------------------------------------------
// GET /api/sessions/:id/events — list session events
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/events",
    params(("id" = Uuid, Path, description = "Agent session ID")),
    responses(
        (status = 200, description = "Session events", body = Vec<AgentEvent>),
        (status = 404, description = "Session not found"),
    ),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn list_session_events(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<AgentEvent>>>, AppError> {
    load_owned_session(&state, &principal, id).await?;
    let events = state
        .store
        .list_agent_events(id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(events))
}

// ---------------------------------------------------------------------------
// GET /api/sessions/:id/messages — convert events to chat messages
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/sessions/{id}/messages",
    params(("id" = Uuid, Path, description = "Agent session ID")),
    responses(
        (status = 200, description = "Reconstructed chat messages", body = SessionMessagesResponse),
        (status = 404, description = "Session not found"),
    ),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn get_session_messages(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SessionMessagesResponse>>, AppError> {
    let session = load_owned_session(&state, &principal, id).await?;

    let events = state
        .store
        .list_agent_events(id)
        .await
        .map_err(AppError::from)?;

    let messages = events_to_messages(&session, &events);
    Ok(ApiResponse::of(SessionMessagesResponse { messages }))
}

// ---------------------------------------------------------------------------
// Event → ChatMessage conversion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SessionMessagesResponse {
    pub messages: Vec<SessionChatMessage>,
}

#[derive(Debug, Clone, Copy, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SessionChatMessage {
    pub role: SessionMessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<SessionToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<SessionUsage>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SessionToolCall {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<std::collections::HashMap<String, Object>>, additional_properties)]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub status: SessionToolCallStatus,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SessionUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Copy, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionToolCallStatus {
    Running,
    Done,
    Error,
    Review,
}

fn events_to_messages(session: &AgentSession, events: &[AgentEvent]) -> Vec<SessionChatMessage> {
    let mut messages: Vec<SessionChatMessage> = Vec::new();

    // First message: the user's original message
    messages.push(SessionChatMessage {
        role: SessionMessageRole::User,
        content: session.user_message.clone(),
        thinking: None,
        tool_calls: Vec::new(),
        usage: None,
    });

    // Build the assistant message from events. The agent emits
    // tool dispatches as `ToolStart` / `ToolComplete` / `ToolError`,
    // and one terminal `Complete` carrying the final text + token
    // usage. `Started` / `Failed` are book-end markers we pass over.
    let mut content = String::new();
    let mut tool_calls: Vec<SessionToolCall> = Vec::new();
    let mut input_tokens: i64 = 0;
    let mut output_tokens: i64 = 0;

    for event in events {
        match &event.payload {
            AgentEventPayload::Started { .. } | AgentEventPayload::Failed { .. } => {}
            AgentEventPayload::ToolStart {
                tool_use_id,
                tool,
                input,
                ..
            } => {
                tool_calls.push(SessionToolCall {
                    id: tool_use_id.clone(),
                    name: tool.clone(),
                    input: Some(input.clone()),
                    output: None,
                    is_error: None,
                    duration_ms: None,
                    reason: None,
                    status: SessionToolCallStatus::Running,
                });
            }
            AgentEventPayload::ToolComplete {
                tool_use_id,
                output,
                duration_ms,
                ..
            } => {
                if let Some(tc) = tool_calls.iter_mut().rev().find(|tc| tc.id == *tool_use_id) {
                    tc.output = Some(output.clone());
                    tc.is_error = Some(false);
                    tc.duration_ms = Some(*duration_ms);
                    tc.status = SessionToolCallStatus::Done;
                }
            }
            AgentEventPayload::ToolError {
                tool_use_id,
                error_for_llm,
                duration_ms,
                ..
            } => {
                if let Some(tc) = tool_calls.iter_mut().rev().find(|tc| tc.id == *tool_use_id) {
                    tc.output = Some(error_for_llm.clone());
                    tc.is_error = Some(true);
                    tc.duration_ms = Some(*duration_ms);
                    tc.reason = Some(error_for_llm.clone());
                    tc.status = SessionToolCallStatus::Error;
                }
            }
            AgentEventPayload::Complete {
                text,
                input_tokens: in_tok,
                output_tokens: out_tok,
                ..
            } => {
                if content.is_empty() {
                    content.push_str(text);
                }
                if let Some(t) = in_tok {
                    input_tokens += *t;
                }
                if let Some(t) = out_tok {
                    output_tokens += *t;
                }
            }
        }
    }

    messages.push(SessionChatMessage {
        role: SessionMessageRole::Assistant,
        content,
        thinking: None,
        tool_calls,
        usage: (input_tokens > 0 || output_tokens > 0).then_some(SessionUsage {
            input_tokens,
            output_tokens,
        }),
    });
    messages
}

// ---------------------------------------------------------------------------
// DELETE /api/sessions/:id — delete a session and its events
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/sessions/{id}",
    params(("id" = Uuid, Path, description = "Agent session ID")),
    responses(
        (status = 204, description = "Session deleted"),
        (status = 404, description = "Session not found"),
    ),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn delete_session(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    // Verify ownership before deleting
    load_owned_session(&state, &principal, id).await?;

    state
        .store
        .delete_agent_session(id)
        .await
        .map_err(AppError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/sessions/:id/tools/:tool_id/respond — HITL tool review
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ToolRespondRequest {
    pub approved: bool,
    pub reason: Option<String>,
    #[schema(value_type = Option<std::collections::HashMap<String, Object>>, additional_properties)]
    pub modified_input: Option<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ToolRespondResponse {
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/api/sessions/{session_id}/tools/{tool_id}/respond",
    params(
        ("session_id" = Uuid, Path, description = "Agent session ID"),
        ("tool_id" = String, Path, description = "Tool call ID awaiting approval"),
    ),
    request_body = ToolRespondRequest,
    responses(
        (status = 200, description = "Approval recorded", body = ToolRespondResponse),
        (status = 403, description = "Caller does not own the session"),
        (status = 404, description = "Session not found"),
    ),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn respond_tool_review(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path((session_id, tool_id)): Path<(Uuid, String)>,
    Json(req): Json<ToolRespondRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ToolRespondResponse>>), AppError> {
    // Verify session exists and belongs to user
    let session = state
        .store
        .get_agent_session(session_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Agent session"))?;

    if session.user_id != principal.id {
        return Err(AppError::resource_not_owned("session"));
    }

    // Persist the approval decision
    let approval = ToolApproval {
        id: Uuid::new_v4(),
        session_id,
        workspace_id: ws.workspace_id,
        tool_call_id: tool_id.clone(),
        approved: req.approved,
        reason: req.reason,
        modified_input: req.modified_input,
        user_id: principal.id.clone(),
        created_at: chrono::Utc::now(),
    };

    state
        .store
        .create_tool_approval(&approval)
        .await
        .map_err(AppError::from)?;

    // Signal the agent's resume channel if registered.
    // If channel doesn't exist yet (race: respond called before SSE registered),
    // the approval is already persisted in DB — the SSE handler will find it
    // via get_tool_approval() before registering a channel.
    if let Some(ref channels) = state.tool_review_channels {
        let key = format!("{session_id}:{tool_id}");
        if let Some((_, sender)) = channels.remove(&key) {
            if sender.send(approval).is_err() {
                tracing::warn!(session_id = %session_id, tool_id = %tool_id, "HITL channel receiver dropped");
            } else {
                tracing::info!(session_id = %session_id, tool_id = %tool_id, "HITL approval delivered via channel");
            }
        } else {
            tracing::info!(session_id = %session_id, tool_id = %tool_id, "HITL approval saved to DB (channel not yet registered)");
        }
    }

    Ok((
        StatusCode::OK,
        ApiResponse::of(ToolRespondResponse {
            status: if req.approved { "approved" } else { "rejected" }.to_string(),
        }),
    ))
}

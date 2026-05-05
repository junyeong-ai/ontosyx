use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use ox_store::{AgentEvent, AgentSession, CursorParams, ToolApproval};

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
    responses((status = 200, description = "Caller's agent sessions", body = Vec<Object>)),
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
        (status = 200, description = "Agent session", body = Object),
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
        (status = 200, description = "Session events", body = Vec<Object>),
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
        (status = 200, description = "Reconstructed chat messages", body = Object),
        (status = 404, description = "Session not found"),
    ),
    security(("api_key" = [])),
    tag = "Sessions",
)]
pub(crate) async fn get_session_messages(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let session = load_owned_session(&state, &principal, id).await?;

    let events = state
        .store
        .list_agent_events(id)
        .await
        .map_err(AppError::from)?;

    let messages = events_to_messages(&session, &events);
    Ok(ApiResponse::of(json!({ "messages": messages })))
}

// ---------------------------------------------------------------------------
// Event → ChatMessage conversion
// ---------------------------------------------------------------------------

fn events_to_messages(session: &AgentSession, events: &[AgentEvent]) -> Vec<serde_json::Value> {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    // First message: the user's original message
    messages.push(json!({
        "role": "user",
        "content": session.user_message,
    }));

    // Build the assistant message from events
    let mut content = String::new();
    let mut thinking = String::new();
    let mut tool_calls: Vec<serde_json::Value> = Vec::new();
    let mut input_tokens: i64 = 0;
    let mut output_tokens: i64 = 0;

    for event in events {
        match event.event_type.as_str() {
            "text" => {
                if let Some(delta) = event.payload.get("delta").and_then(|v| v.as_str()) {
                    content.push_str(delta);
                }
            }
            "thinking" => {
                if let Some(text) = event.payload.get("content").and_then(|v| v.as_str()) {
                    if !thinking.is_empty() {
                        thinking.push('\n');
                    }
                    thinking.push_str(text);
                }
            }
            "tool_start" => {
                tool_calls.push(json!({
                    "id": event.payload.get("id"),
                    "name": event.payload.get("name"),
                    "input": event.payload.get("input"),
                    "status": "running",
                }));
            }
            "tool_complete" => {
                let tool_id = event.payload.get("id");
                if let Some(tc) = tool_calls
                    .iter_mut()
                    .rev()
                    .find(|tc| tc.get("id") == tool_id)
                {
                    tc["output"] = event.payload.get("output").cloned().unwrap_or(json!(null));
                    tc["is_error"] = event
                        .payload
                        .get("is_error")
                        .cloned()
                        .unwrap_or(json!(false));
                    tc["duration_ms"] = event
                        .payload
                        .get("duration_ms")
                        .cloned()
                        .unwrap_or(json!(null));
                    tc["status"] = json!("complete");
                }
            }
            "tool_review" => {
                tool_calls.push(json!({
                    "id": event.payload.get("id"),
                    "name": event.payload.get("name"),
                    "input": event.payload.get("input"),
                    "status": "review",
                }));
            }
            "tool_blocked" => {
                tool_calls.push(json!({
                    "id": event.payload.get("id"),
                    "name": event.payload.get("name"),
                    "reason": event.payload.get("reason"),
                    "status": "blocked",
                }));
            }
            "turn_usage" => {
                if let Some(n) = event.payload.get("input_tokens").and_then(|v| v.as_i64()) {
                    input_tokens += n;
                }
                if let Some(n) = event.payload.get("output_tokens").and_then(|v| v.as_i64()) {
                    output_tokens += n;
                }
            }
            "complete" => {
                // The complete event's text is the final accumulated text;
                // we already built content from text deltas, so we use that.
                // If content is empty but complete has text, use it as fallback.
                if content.is_empty()
                    && let Some(text) = event.payload.get("text").and_then(|v| v.as_str())
                {
                    content.push_str(text);
                }
            }
            _ => {}
        }
    }

    // Build the assistant message
    let mut assistant = json!({
        "role": "assistant",
        "content": content,
    });

    if !thinking.is_empty() {
        assistant["thinking"] = json!(thinking);
    }
    if !tool_calls.is_empty() {
        assistant["tool_calls"] = json!(tool_calls);
    }
    if input_tokens > 0 || output_tokens > 0 {
        assistant["usage"] = json!({
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        });
    }

    messages.push(assistant);
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
    #[schema(value_type = Option<Object>)]
    pub modified_input: Option<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ToolRespondResponse {
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/api/sessions/{id}/tools/{tool_id}/respond",
    params(
        ("id" = Uuid, Path, description = "Agent session ID"),
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

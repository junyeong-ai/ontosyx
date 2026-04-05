use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    response::sse::{Event, Sse},
};
use chrono::Utc;
use futures_core::Stream;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tracing::info;
use uuid::Uuid;

use branchforge::{AgentEvent, ExecutionMode};
use ox_agent::{BuildAgentResult, DomainContext, OntosyxAgentConfig, build_agent};
use ox_core::ontology_ir::OntologyIR;
use ox_store::AgentSession;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::validation;

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ChatStreamRequest {
    pub message: String,
    #[schema(value_type = Object)]
    pub ontology: OntologyIR,
    #[serde(default)]
    pub saved_ontology_id: Option<Uuid>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub project_revision: Option<i32>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub execution_mode: Option<String>,
    /// Override the LLM model for this request (e.g., "claude-opus-4-6").
    #[serde(default)]
    pub model_override: Option<String>,
}

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

fn validate_chat_stream_request(req: &ChatStreamRequest) -> Result<(), AppError> {
    validation::validate_message("message", &req.message)?;
    validation::validate_ontology_input(&req.ontology)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// POST /api/chat/stream — branchforge Agent SSE streaming
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/chat/stream",
    request_body = ChatStreamRequest,
    responses(
        (status = 200, description = "SSE stream: Agent events", content_type = "text/event-stream"),
        (status = 400, description = "Invalid request", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Chat",
)]
#[tracing::instrument(skip(state, principal, req), fields(session_id))]
pub async fn chat_stream(
    State(state): State<AppState>,
    principal: Principal,
    ws: crate::workspace::WorkspaceContext,
    Json(req): Json<ChatStreamRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    validate_chat_stream_request(&req)?;

    let user_message = req.message.clone();
    let ontology = req.ontology.clone();
    let user_id = principal.id.clone();
    let is_system = principal.is_system();
    // Capture workspace context NOW (while middleware scope is active).
    // This must be used for ALL spawn calls inside the SSE stream,
    // because the stream runs AFTER the middleware scope ends.
    let ws_scope = crate::spawn_scoped::WsScope::capture();
    let ws_id = ws.workspace_id;
    let model_id = state.brain.default_model_info().model.clone();

    // Load source schema + repo insights from project (deserialize JSONB → typed structs)
    let (source_schema, source_profile, repo_insights) = if let Some(project_id) = req.project_id {
        match state.store.get_design_project(project_id).await {
            Ok(Some(project)) => {
                let schema = project
                    .source_schema
                    .as_ref()
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let profile = project
                    .source_profile
                    .as_ref()
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let insights = project
                    .analysis_report
                    .as_ref()
                    .and_then(|r| r.get("repo_summary"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                (schema, profile, insights)
            }
            _ => (None, None, None),
        }
    } else {
        (None, None, None)
    };

    // Build domain context
    let domain = Arc::new(DomainContext {
        compiler: Arc::clone(&state.compiler),
        runtime: state.runtime.clone(),
        store: Arc::clone(&state.store),
        ontology: Some(ontology),
        user_id: user_id.clone(),
        workspace_id: ws.workspace_id,
        saved_ontology_id: req.saved_ontology_id,
        project_id: req.project_id,
        project_revision: req.project_revision,
        source_schema,
        source_profile,
        repo_insights,
        knowledge_store: Some(Arc::clone(&state.store) as Arc<dyn ox_store::KnowledgeStore>),
        user_question: Some(user_message.clone()),
    });

    // Parse execution mode from request
    let execution_mode = match req.execution_mode.as_deref() {
        Some("supervised") => ExecutionMode::Supervised,
        _ => ExecutionMode::Auto,
    };

    // Build agent
    let requested_session_id = req.session_id.clone();
    let BuildAgentResult {
        agent,
        session_resumed,
    } = build_agent(OntosyxAgentConfig {
        auth: state.agent_auth.clone(),
        model: model_id.clone(),
        execution_mode,
        domain: Arc::clone(&domain),
        brain: Arc::clone(&state.brain),
        memory: state.memory.clone(),
        session_id: requested_session_id.clone(),
        user_role: principal.role.as_str().to_string(),
    })
    .await
    .map_err(|e| AppError::internal(format!("Agent initialization failed: {e}")))?;

    // Propagate workspace context into agent tool execution futures.
    // This ensures task-locals (WORKSPACE_ID, SYSTEM_BYPASS) are available
    // inside parallel tool calls spawned by branchforge.
    let workspace_scope: std::sync::Arc<dyn branchforge::ContextScope> = if is_system {
        std::sync::Arc::new(crate::workspace_scope::WorkspaceContextScope::SystemBypass)
    } else {
        std::sync::Arc::new(crate::workspace_scope::WorkspaceContextScope::Workspace {
            workspace_id: ws.workspace_id,
        })
    };
    let ws_scope_for_stream = workspace_scope.clone();
    let agent = agent.with_context_scope(workspace_scope);

    // Detect session expiry: caller sent a session_id but resume failed.
    let session_expired = requested_session_id.is_some() && !session_resumed;

    // Compute hashes for replay/audit
    let system_prompt = ox_agent::system_prompt_text(&domain, principal.role.as_str()).await;
    let prompt_hash = sha256_hex(system_prompt.as_bytes());
    let tool_schema_hash = compute_tool_schema_hash(&agent);

    // Create audit session record
    let audit_session_id = Uuid::new_v4();
    let audit_session = AgentSession {
        id: audit_session_id,
        user_id: user_id.clone(),
        ontology_id: req.ontology.id.clone().into(),
        prompt_hash,
        tool_schema_hash,
        model_id: model_id.clone(),
        model_config: serde_json::json!({"execution_mode": req.execution_mode.as_deref().unwrap_or("auto")}),
        user_message: user_message.clone(),
        final_text: None,
        created_at: Utc::now(),
        completed_at: None,
    };

    // Fire-and-forget session creation (non-blocking).
    // spawn_scoped automatically propagates SYSTEM_BYPASS/WORKSPACE_ID task-locals.
    let store_for_session = Arc::clone(&state.store);
    crate::spawn_scoped::spawn_with_ws(ws_scope.clone(), {
        let session = audit_session.clone();
        async move {
            if let Err(e) = store_for_session.create_agent_session(&session).await {
                tracing::warn!(error = %e, "Failed to create agent session record");
            }
        }
    });

    // NOTE: The branchforge agent runs in its own tokio task during SSE streaming.
    // Task-locals (WORKSPACE_ID, SYSTEM_BYPASS) do NOT propagate to agent-internal
    // DB calls (schema_rag, query_persist, memory embedding).
    // For JWT users: WORKSPACE_ID is set in before_acquire → works correctly.
    // For API key users: SYSTEM_BYPASS is lost → agent DB calls fallback to
    // no-context (RLS deny-all). This causes non-critical warnings but does NOT
    // affect the chat response or query execution. The agent still functions
    // correctly — only audit/embedding persistence is skipped.
    // Full fix requires workspace_id propagation into the branchforge agent.

    info!(
        user_id = %principal.id,
        audit_session_id = %audit_session_id,
        message_len = user_message.len(),
        "Agent chat stream started"
    );

    // Capture ontology_id for embedding scoping in the stream closure
    let ontology_id_for_stream = req.saved_ontology_id.map(|id| id.to_string());

    // Capture values for metering inside the stream closure
    let principal_user_uuid = principal.user_uuid().ok();
    let model_id_for_stream = model_id.clone();

    // Stream agent events as SSE
    let store_for_events = Arc::clone(&state.store);
    let stream = async_stream::stream! {
        // Notify the client when a requested session could not be resumed.
        if session_expired {
            let expired_id = requested_session_id.as_deref().unwrap_or("");
            yield Ok(Event::default().event("session_expired").data(
                serde_json::json!({
                    "previous_session_id": expired_id,
                    "message": "Session expired. Starting a new session."
                }).to_string()
            ));
        }

        let mut rc = branchforge::RunConfig::new();
        if let Some(model) = &req.model_override {
            rc = rc.model(model);
        }
        let execute_result = agent.execute_stream_with(&user_message, rc).await;

        match execute_result {
            Ok(event_stream) => {
                let mut event_stream = std::pin::pin!(event_stream);
                let memory_for_stream = state.memory.clone();
                let mut event_sequence: i32 = 0;

                while let Some(event_result) = event_stream.next().await {
                    {
                    match event_result {
                        Ok(ref agent_event) => {
                            // Record event for audit (fire-and-forget)
                            event_sequence += 1;
                            if let Some(sse_event) = agent_event_to_sse(agent_event) {
                                let audit_event = ox_store::AgentEvent {
                                    id: Uuid::new_v4(),
                                    session_id: audit_session_id,
                                    workspace_id: ws_id,
                                    sequence: event_sequence,
                                    event_type: agent_event.event_type().to_string(),
                                    payload: serde_json::to_value(agent_event).unwrap_or_default(),
                                    created_at: Utc::now(),
                                };
                                let store = Arc::clone(&store_for_events);
                                crate::spawn_scoped::spawn_with_ws(ws_scope.clone(), async move {
                                    let _ = store.create_agent_event(&audit_event).await;
                                });

                                yield Ok(sse_event);
                            }

                            // Record usage metering for cost tracking (fire-and-forget)
                            if let AgentEvent::TurnUsage { input_tokens, output_tokens, .. } = &agent_event {
                                let meter_store = Arc::clone(&store_for_events);
                                let meter_user_id = principal_user_uuid;
                                let meter_model = model_id_for_stream.clone();
                                let in_tok = *input_tokens as i64;
                                let out_tok = *output_tokens as i64;
                                crate::spawn_scoped::spawn_with_ws(ws_scope.clone(), async move {
                                    let fut = meter_store.record_usage(
                                        meter_user_id,
                                        "llm",
                                        Some("anthropic"),
                                        Some(&meter_model),
                                        Some("chat"),
                                        in_tok,
                                        out_tok,
                                        0, // duration not available per-turn
                                        0.0, // cost computed by aggregation layer
                                        serde_json::json!({}),
                                    );
                                    let _ = fut.await;
                                });
                            }

                            // HITL: when a tool review event is emitted, register a
                            // oneshot channel and wait for the user's approval.
                            if let AgentEvent::ToolReview { id, .. } = agent_event
                                && let Some(ref channels) = state.tool_review_channels {
                                    let key = format!("{audit_session_id}:{id}");

                                    // Race condition recovery: check if approval arrived
                                    // before channel was registered (saved to DB by respond_tool_review).
                                    if let Ok(Some(existing)) = state.store.get_tool_approval(audit_session_id, id).await {
                                        tracing::info!(session_id = %audit_session_id, tool_id = %id, "Tool approval found in DB (pre-registered)");
                                        let status = if existing.approved { "approved" } else { "rejected" };
                                        yield Ok(Event::default().event("tool_review_result").data(
                                            serde_json::json!({
                                                "tool_call_id": id,
                                                "status": status,
                                                "reason": existing.reason,
                                            }).to_string()
                                        ));
                                    } else {
                                        // Normal path: register channel and wait
                                        let (tx, rx) = tokio::sync::oneshot::channel();
                                        channels.insert(key.clone(), tx);
                                        tracing::info!(session_id = %audit_session_id, tool_id = %id, "HITL channel registered, awaiting approval");

                                        let timeout_secs = state.system_config.read().await.tool_review_timeout_secs();
                                        match tokio::time::timeout(
                                            std::time::Duration::from_secs(timeout_secs),
                                            rx,
                                        ).await {
                                            Ok(Ok(approval)) => {
                                                let status = if approval.approved { "approved" } else { "rejected" };
                                                tracing::info!(session_id = %audit_session_id, tool_id = %id, %status, "Tool review resolved");
                                                yield Ok(Event::default().event("tool_review_result").data(
                                                    serde_json::json!({
                                                        "tool_call_id": id,
                                                        "status": status,
                                                        "reason": approval.reason,
                                                    }).to_string()
                                                ));
                                            }
                                            _ => {
                                                tracing::warn!(session_id = %audit_session_id, tool_id = %id, timeout_secs, "Tool review timed out");
                                                yield Ok(Event::default().event("tool_review_result").data(
                                                    serde_json::json!({
                                                        "tool_call_id": id,
                                                        "status": "timeout",
                                                    }).to_string()
                                                ));
                                            }
                                        }
                                        channels.remove(&key);
                                    }
                                }

                            // On completion: embed session summary + complete audit session
                            if let AgentEvent::Complete(result) = agent_event {
                                if let Some(ref memory) = memory_for_stream
                                    && !result.text.is_empty() {
                                        ox_agent::hooks::EmbeddingHook::embed_async(
                                            memory,
                                            result.text.clone(),
                                            ox_memory::MemorySource::Session,
                                            ontology_id_for_stream.clone(),
                                            Some(result.session_id.clone()),
                                            None, // session summaries: no retry
                                            Some(ws_scope_for_stream.clone()),
                                        );
                                    }

                                // Complete audit session
                                let store = Arc::clone(&store_for_events);
                                let final_text = result.text.clone();
                                crate::spawn_scoped::spawn_with_ws(ws_scope.clone(), async move {
                                    let fut = store.complete_agent_session(
                                        audit_session_id,
                                        Some(&final_text),
                                    );
                                    let _ = fut.await;
                                });
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Agent event stream error");
                            yield Ok(Event::default().event("error").data(
                                serde_json::json!({
                                    "error": { "type": "agent_error", "message": format!("{e}") }
                                }).to_string()
                            ));
                            return;
                        }
                    }
                    } // end match block
                } // end while
            }
            Err(e) => {
                tracing::error!(error = %e, "execute_stream() failed");
                yield Ok(Event::default().event("error").data(
                    serde_json::json!({
                        "error": { "type": "agent_error", "message": format!("{e}") }
                    }).to_string()
                ));
            }
        }
    };

    Ok(Sse::new(stream))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn compute_tool_schema_hash(agent: &branchforge::Agent) -> String {
    let tool_names = agent.tools().names();
    let mut sorted = tool_names;
    sorted.sort();
    let definitions: Vec<serde_json::Value> = agent
        .tools()
        .definitions()
        .into_iter()
        .map(|d| serde_json::json!({"name": d.name, "description": d.description, "schema": d.input_schema}))
        .collect();
    sha256_hex(
        serde_json::to_string(&definitions)
            .unwrap_or_default()
            .as_bytes(),
    )
}

/// Convert branchforge AgentEvent to Axum SSE Event.
///
/// SSE uses the event type as the SSE `event` field and a lightweight
/// JSON payload as `data`. For Complete events, only summary fields are
/// sent to avoid streaming full messages/metrics to the client.
fn agent_event_to_sse(event: &AgentEvent) -> Option<Event> {
    let (event_name, data) = match event {
        AgentEvent::Text { delta } => ("text", serde_json::json!({ "delta": delta })),
        AgentEvent::Thinking { content } => ("thinking", serde_json::json!({ "content": content })),
        AgentEvent::ToolStart { id, name, input } => (
            "tool_start",
            serde_json::json!({ "id": id, "name": name, "input": input }),
        ),
        AgentEvent::ToolComplete {
            id,
            name,
            output,
            is_error,
            duration_ms,
        } => (
            "tool_complete",
            serde_json::json!({ "id": id, "name": name, "output": output, "is_error": is_error, "duration_ms": duration_ms }),
        ),
        AgentEvent::ToolProgress {
            id,
            name: _,
            step,
            status,
            timestamp: _,
            duration_ms,
            metadata,
        } => (
            "tool_progress",
            serde_json::json!({
                "tool_call_id": id,
                "step": step,
                "status": status,
                "duration_ms": duration_ms,
                "metadata": metadata,
            }),
        ),
        AgentEvent::ToolBlocked { id, name, reason } => (
            "tool_blocked",
            serde_json::json!({ "id": id, "name": name, "reason": reason }),
        ),
        AgentEvent::ToolReview { id, name, input } => (
            "tool_review",
            serde_json::json!({ "id": id, "name": name, "input": input }),
        ),
        AgentEvent::TurnUsage {
            input_tokens,
            output_tokens,
            ..
        } => (
            "usage",
            serde_json::json!({ "input_tokens": input_tokens, "output_tokens": output_tokens }),
        ),
        AgentEvent::Complete(result) => (
            "complete",
            serde_json::json!({
                "session_id": result.session_id,
                "text": result.text,
                "tool_calls": result.tool_calls,
                "iterations": result.iterations,
            }),
        ),
    };

    Some(Event::default().event(event_name).data(data.to_string()))
}

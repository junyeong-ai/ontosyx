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
use ox_brain::model_resolver::{ModelResolver, operation};
use ox_ontology::ir::OntologyIR;
use ox_store::{AgentEventPayload, AgentExecutionMode, AgentSession, AgentSessionModelConfig};

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
    pub ontology: OntologyIR,
    #[serde(default)]
    pub ontology_id: Option<Uuid>,
    #[serde(default)]
    pub ontology_draft_id: Option<Uuid>,
    #[serde(default)]
    pub ontology_draft_revision: Option<i32>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub execution_mode: Option<AgentExecutionMode>,
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
pub(crate) async fn chat_stream(
    State(state): State<AppState>,
    principal: Principal,
    ws: crate::workspace::WorkspaceContext,
    Json(req): Json<ChatStreamRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    validate_chat_stream_request(&req)?;

    // Acquire a per-user chat-stream slot BEFORE we start setting up the
    // stream. Rejecting early keeps the ceiling enforced even under the
    // slowest-possible-handler-startup attack and gives the client a
    // structured 429 instead of a dropped SSE connection.
    //
    // The `StreamSlot` guard lives through the SSE stream — `async_stream`
    // will drop it when the handler future is cancelled (client
    // disconnect or agent completion), releasing the permit for the next
    // request from the same user.
    let stream_slot = match state.stream_limiter.try_acquire(&principal.id) {
        Some(slot) => slot,
        None => {
            return Err(AppError::too_many_requests(format!(
                "Chat stream concurrency cap reached ({} simultaneous streams per user). \
                 Close an existing conversation before starting a new one, or raise \
                 `agent.max_concurrent_streams_per_user` in config.",
                state.stream_limiter.max_per_user(),
            )));
        }
    };

    let user_message = req.message.clone();
    let ontology = req.ontology.clone();
    let user_id = principal.id.clone();
    let is_system = principal.is_machine();
    // Capture workspace context NOW (while middleware scope is active).
    // This must be used for ALL spawn calls inside the SSE stream,
    // because the stream runs AFTER the middleware scope ends.
    let ws_scope = crate::spawn_scoped::WsScope::capture();
    let ws_id = ws.workspace_id;
    let resolved_chat_model = state
        .model_router
        .resolve(operation::CHAT)
        .await
        .map_err(AppError::from)?;
    let model_id = req
        .model_override
        .clone()
        .unwrap_or_else(|| resolved_chat_model.model_id.clone());
    let model_provider = resolved_chat_model.provider.clone();
    let agent_auth = resolved_chat_model
        .provider_config
        .as_ref()
        .ok_or_else(|| {
            AppError::service_unavailable(
                "LLM provider unavailable. Run `./scripts/dev.sh health` or POST /api/models/test for the provider diagnostic.",
            )
        })?
        .resolve_auth()
        .map_err(|error| {
            tracing::warn!(%error, provider = %model_provider, "Agent auth resolution failed");
            AppError::service_unavailable(
                "LLM provider unavailable. Run `./scripts/dev.sh health` or POST /api/models/test for the provider diagnostic.",
            )
        })?;
    let resolved_ontology_id = if req.ontology_id.is_some() || req.ontology_draft_id.is_some() {
        req.ontology_id
    } else {
        state
            .store
            .get_workspace_ontology()
            .await
            .map_err(AppError::from)?
            .map(|identity| identity.id)
    };

    // Load source schema + repo insights from project (deserialize JSONB → typed structs)
    let (source_schema, source_profile, repo_insights) =
        if let Some(ontology_draft_id) = req.ontology_draft_id {
            match state.store.get_ontology_draft(ontology_draft_id).await {
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
        ontology: Some(arc_swap::ArcSwap::from_pointee(ontology)),
        user_id: user_id.clone(),
        workspace_id: ws.workspace_id,
        ontology_id: resolved_ontology_id,
        ontology_draft_id: req.ontology_draft_id,
        ontology_draft_revision: req.ontology_draft_revision,
        source_schema,
        source_profile,
        repo_insights,
        knowledge_store: Some(Arc::clone(&state.store) as Arc<dyn ox_store::KnowledgeStore>),
        ambiguity_store: Some(Arc::clone(&state.store) as Arc<dyn ox_store::AmbiguityStore>),
        clarification_tracker: Arc::clone(&state.clarification_tracker),
        user_question: Some(user_message.clone()),
        tokenizer_registry: Some(Arc::clone(&state.tokenizer_registry)),
        embedder: state.memory.as_ref().map(|m| Arc::clone(m.embedder())),
    });

    // Parse execution mode from request
    let platform_execution_mode = req.execution_mode.unwrap_or(AgentExecutionMode::Auto);
    let execution_mode = branchforge_execution_mode(platform_execution_mode);

    // Build agent
    let requested_session_id = req.session_id.clone();
    let BuildAgentResult {
        agent,
        session_resumed,
    } = build_agent(OntosyxAgentConfig {
        auth: agent_auth,
        model: model_id.clone(),
        execution_mode,
        domain: Arc::clone(&domain),
        brain: Arc::clone(&state.brain),
        memory: state.memory.clone(),
        session_id: requested_session_id.clone(),
        user_role: principal.role.as_str().to_string(),
        recovery: state.recovery_hook_config(),
        max_iterations: state.agent.max_iterations,
        reject_high_cost: state.agent.reject_high_cost,
    })
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "Agent initialization failed");
        AppError::service_unavailable(
            "LLM provider unavailable. Run `./scripts/dev.sh health` or POST /api/models/test for the provider diagnostic.",
        )
    })?;

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
        ontology_lineage_id: req.ontology.id.clone().into(),
        prompt_hash,
        tool_schema_hash,
        model_id: model_id.clone(),
        model_config: AgentSessionModelConfig {
            execution_mode: platform_execution_mode,
        },
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
    let ontology_id_for_stream = resolved_ontology_id.map(|id| id.to_string());

    // Capture values for metering inside the stream closure
    let principal_user_uuid = principal.user_uuid().ok();
    let model_id_for_stream = model_id.clone();
    let model_provider_for_stream = model_provider.clone();
    // Online-sampling capture — moved into the stream so the
    // post-completion sample includes the original user
    // question without re-borrowing `req`.
    let user_message_for_stream = user_message.clone();

    // Stream agent events as SSE
    let store_for_events = Arc::clone(&state.store);
    // Move the concurrency-cap guard into the stream body. It releases
    // its permit when the stream terminates (cancel, complete, or drop),
    // which is exactly the lifetime we want to bound.
    let _stream_slot = stream_slot;
    // Clone the captured ws_scope for the outer wrapper before the
    // stream body moves it via `spawn_with_ws`.
    let outer_ws_scope = ws_scope.clone();
    let stream = async_stream::stream! {
        // Keep the permit alive for the stream's lifetime — capturing it
        // by move into the generator means the `Drop` fires when the SSE
        // stream ends, not when the handler returns to axum.
        let _stream_slot = _stream_slot;
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

        let rc = branchforge::RunConfig::new();
        let execute_result = agent.execute_stream_with(&user_message, rc).await;

        match execute_result {
            Ok(event_stream) => {
                let mut event_stream = std::pin::pin!(event_stream);
                let memory_for_stream = state.memory.clone();
                let mut event_sequence: i32 = 0;

                // Wall-clock ceiling on the entire agent loop. `max_iterations`
                // (ox-agent build) caps planner turns; this cap protects the
                // workspace when a single turn (deep analysis, large
                // introspection) stalls indefinitely. Once the deadline is
                // reached we surface one `error` SSE event and break out
                // cleanly so the client sees a bounded failure instead of a
                // silently-held connection.
                let stream_deadline =
                    tokio::time::Instant::now() + state.timeouts.chat_wall_clock;

                while let Some(event_result) = event_stream.next().await {
                    if tokio::time::Instant::now() >= stream_deadline {
                        tracing::warn!(
                            session_id = %audit_session_id,
                            wall_clock_secs = state.timeouts.chat_wall_clock.as_secs(),
                            "Chat stream exceeded wall-clock budget — terminating"
                        );
                        yield Ok(Event::default().event("error").data(
                            serde_json::json!({
                                "type": "timeout",
                                "message": format!(
                                    "Agent loop exceeded {}s ceiling. Shorten the question or raise `timeouts.chat_wall_clock_secs` in config.",
                                    state.timeouts.chat_wall_clock.as_secs()
                                ),
                            }).to_string()
                        ));
                        break;
                    }
                    {
                    match event_result {
                        Ok(ref agent_event) => {
                            // Record event for audit (fire-and-forget)
                            event_sequence += 1;
                            if let Some(payload) = agent_event_payload(agent_event) {
                                let audit_event = ox_store::AgentEvent {
                                    id: Uuid::new_v4(),
                                    session_id: audit_session_id,
                                    workspace_id: ws_id,
                                    sequence: event_sequence,
                                    event_type: payload.event_type().to_string(),
                                    payload: payload.clone(),
                                    created_at: Utc::now(),
                                };
                                let store = Arc::clone(&store_for_events);
                                crate::spawn_scoped::spawn_with_ws(ws_scope.clone(), async move {
                                    if let Err(error) = store.create_agent_event(&audit_event).await {
                                        tracing::warn!(?error, "agent audit event emit failed");
                                    }
                                });

                                yield Ok(agent_payload_to_sse(&payload));
                            }

                            // Record usage metering for cost tracking (fire-and-forget)
                            if let AgentEvent::TurnUsage { input_tokens, output_tokens, .. } = &agent_event {
                                let meter_store = Arc::clone(&store_for_events);
                                let meter_user_id = principal_user_uuid;
                                let meter_model = model_id_for_stream.clone();
                                let meter_provider = model_provider_for_stream.clone();
                                let in_tok = input_tokens.get() as i64;
                                let out_tok = output_tokens.get() as i64;
                                crate::spawn_scoped::spawn_with_ws(ws_scope.clone(), async move {
                                    let fut = meter_store.record_usage(
                                        meter_user_id,
                                        "llm",
                                        Some(&meter_provider),
                                        Some(&meter_model),
                                        Some(operation::CHAT),
                                        in_tok,
                                        out_tok,
                                        0, // duration not available per-turn
                                        0.0, // cost computed by aggregation layer
                                        serde_json::json!({}),
                                    );
                                    if let Err(error) = fut.await {
                                        tracing::warn!(?error, "usage metering record failed");
                                    }
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
                                            ws_scope_for_stream.clone(),
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
                                    if let Err(error) = fut.await {
                                        tracing::warn!(?error, %audit_session_id, "agent session completion record failed");
                                    }
                                });

                                // Online evaluation sampler — at the
                                // configured rate, drop a sample
                                // (question, final answer, model)
                                // into the workspace's
                                // `live_chat_samples` evaluation
                                // run. Async judge worker picks
                                // them up in the background. Best-
                                // effort; spawned task isolates
                                // store calls from the SSE stream
                                // path so a sampler failure never
                                // delays the user's response.
                                if !result.text.is_empty() {
                                    crate::eval_sampler::spawn_sample(
                                        Arc::clone(&store_for_events),
                                        ws_scope.clone(),
                                        crate::eval_sampler::sampling_config_from_env(),
                                        crate::eval_sampler::ChatSampleInput {
                                            workspace_id: ws_id,
                                            question: user_message_for_stream.clone(),
                                            answer: result.text.clone(),
                                            model_id: model_id_for_stream.clone(),
                                        },
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Agent event stream error");
                            yield Ok(Event::default().event("error").data(
                                serde_json::json!({
                                    "error": {
                                        "code": "agent_error",
                                        "class": "server_error",
                                        "params": { "detail": format!("{e}") }
                                    }
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
                        "error": {
                            "code": "agent_error",
                            "class": "server_error",
                            "params": { "detail": format!("{e}") }
                        }
                    }).to_string()
                ));
            }
        }
    };

    // Wrap the stream so per-poll store reads inside the body
    // re-enter `WORKSPACE_ID` / `SYSTEM_BYPASS` task-locals (axum
    // drives the Stream after the request middleware's scope has
    // already exited).
    Ok(Sse::new(crate::spawn_scoped::scope_stream(
        outer_ws_scope,
        stream,
    )))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
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

fn branchforge_execution_mode(mode: AgentExecutionMode) -> ExecutionMode {
    match mode {
        AgentExecutionMode::Auto => ExecutionMode::Auto,
        AgentExecutionMode::Plan => ExecutionMode::Plan,
        AgentExecutionMode::Supervised => ExecutionMode::Supervised,
    }
}

/// Convert branchforge AgentEvent to the platform-owned persisted
/// payload. Events without a client-facing representation are not
/// persisted in the session timeline.
fn agent_event_payload(event: &AgentEvent) -> Option<AgentEventPayload> {
    Some(match event {
        AgentEvent::Text { delta } => AgentEventPayload::Text {
            delta: delta.clone(),
        },
        AgentEvent::Thinking { content } => AgentEventPayload::Thinking {
            content: content.clone(),
        },
        AgentEvent::ToolStart { id, name, input } => AgentEventPayload::ToolStart {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        AgentEvent::ToolComplete {
            id,
            name,
            output,
            is_error,
            duration_ms,
        } => AgentEventPayload::ToolComplete {
            id: id.clone(),
            name: name.clone(),
            output: serde_json::Value::String(output.clone()),
            is_error: *is_error,
            duration_ms: Some(*duration_ms as i64),
        },
        AgentEvent::ToolProgress {
            id,
            name: _,
            step,
            status,
            timestamp: _,
            duration_ms,
            metadata,
        } => AgentEventPayload::ToolProgress {
            tool_call_id: id.clone(),
            step: step.clone(),
            status: format!("{status:?}"),
            duration_ms: duration_ms.map(|ms| ms as i64),
            metadata: metadata.clone().unwrap_or(serde_json::Value::Null),
        },
        AgentEvent::ToolBlocked { id, name, reason } => AgentEventPayload::ToolBlocked {
            id: id.clone(),
            name: name.clone(),
            reason: reason.clone(),
        },
        AgentEvent::ToolReview { id, name, input } => AgentEventPayload::ToolReview {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        AgentEvent::TurnUsage {
            input_tokens,
            output_tokens,
            ..
        } => AgentEventPayload::TurnUsage {
            input_tokens: input_tokens.get() as i64,
            output_tokens: output_tokens.get() as i64,
        },
        AgentEvent::Complete(result) => AgentEventPayload::Complete {
            session_id: result.session_id.clone(),
            text: result.text.clone(),
            tool_calls: serde_json::to_value(result.tool_calls).unwrap_or_default(),
            iterations: result.iterations as u32,
        },
        _ => return None,
    })
}

/// Convert the persisted payload to Axum SSE. SSE uses the historic
/// `usage` event name for token usage while the stored timeline keeps
/// the canonical `turn_usage` type.
fn agent_payload_to_sse(payload: &AgentEventPayload) -> Event {
    let (event_name, data) = match payload {
        AgentEventPayload::Text { delta } => ("text", serde_json::json!({ "delta": delta })),
        AgentEventPayload::Thinking { content } => {
            ("thinking", serde_json::json!({ "content": content }))
        }
        AgentEventPayload::ToolStart { id, name, input } => (
            "tool_start",
            serde_json::json!({ "id": id, "name": name, "input": input }),
        ),
        AgentEventPayload::ToolComplete {
            id,
            name,
            output,
            is_error,
            duration_ms,
        } => (
            "tool_complete",
            serde_json::json!({ "id": id, "name": name, "output": output, "is_error": is_error, "duration_ms": duration_ms }),
        ),
        AgentEventPayload::ToolProgress {
            tool_call_id,
            step,
            status,
            duration_ms,
            metadata,
        } => (
            "tool_progress",
            serde_json::json!({
                "tool_call_id": tool_call_id,
                "step": step,
                "status": status,
                "duration_ms": duration_ms,
                "metadata": metadata,
            }),
        ),
        AgentEventPayload::ToolBlocked { id, name, reason } => (
            "tool_blocked",
            serde_json::json!({ "id": id, "name": name, "reason": reason }),
        ),
        AgentEventPayload::ToolReview { id, name, input } => (
            "tool_review",
            serde_json::json!({ "id": id, "name": name, "input": input }),
        ),
        AgentEventPayload::TurnUsage {
            input_tokens,
            output_tokens,
        } => (
            "usage",
            serde_json::json!({ "input_tokens": input_tokens, "output_tokens": output_tokens }),
        ),
        AgentEventPayload::Complete {
            session_id,
            text,
            tool_calls,
            iterations,
        } => (
            "complete",
            serde_json::json!({
                "session_id": session_id,
                "text": text,
                "tool_calls": tool_calls,
                "iterations": iterations,
            }),
        ),
    };

    Event::default().event(event_name).data(data.to_string())
}

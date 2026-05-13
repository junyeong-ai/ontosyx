use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    response::sse::{Event, Sse},
};
use chrono::Utc;
use entelix::ChannelSink;
use entelix::{AgentEvent, ReActState};
use futures_core::Stream;
use ox_agent::{
    BuildAgentRequest, BuildAgentResult, DomainContext, build_agent, build_execution_context,
};
use ox_brain::auth::LlmProviderConfig;
use ox_brain::model_resolver::{ModelResolver, operation};
use ox_context::{ContextScope, WorkspaceMode};
use ox_ontology::ir::OntologyIR;
use ox_store::{AgentEventPayload, AgentSession};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use tracing::info;
use uuid::Uuid;

use crate::agent_event_projection::project_agent_event;
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
    /// Override the LLM model for this request (e.g., "claude-opus-4-7").
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
// POST /api/chat/stream — agent SSE streaming over entelix
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
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn chat_stream(
    State(state): State<AppState>,
    principal: Principal,
    ws: crate::workspace::WorkspaceContext,
    Json(req): Json<ChatStreamRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    validate_chat_stream_request(&req)?;

    // Acquire a per-user chat-stream slot BEFORE setting the stream up.
    // Rejecting early gives the client a structured 429 instead of a
    // dropped SSE connection.
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
    // SSE streaming polls this future after the middleware scope has
    // exited, so the captured scope is what `scope_stream` re-applies
    // around every `poll_next` call.
    let ws_scope = ContextScope::capture_current();
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
    let provider_config = build_provider_config(&resolved_chat_model, &model_id).ok_or_else(|| {
        tracing::warn!(provider = %model_provider, "model router returned no provider config");
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

    // Source schema + repo insights from the open draft (if any).
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

    let workspace_mode = if is_system {
        WorkspaceMode::SystemBypass
    } else {
        WorkspaceMode::Workspace(ws.workspace_id)
    };

    // Per-request channel sink — every event the agent emits flows
    // through here (book-end + tool variants alike). The fan-out
    // adapter inside `build_agent` clones every event into the
    // domain sinks (`EmbeddingSink`, `RecoveryDetectionSink`) and
    // into this sink; the SSE handler polls the matching receiver.
    // 256 capacity is generous — one ReAct turn typically emits
    // ≤10 events even with several tool calls.
    let (sse_sink, mut sse_rx) = ChannelSink::<ReActState>::new(256);

    let BuildAgentResult {
        agent,
        tool_schema_hash,
    } = build_agent(BuildAgentRequest {
        provider_config,
        chat_model_registry: Arc::clone(&state.chat_model_registry),
        domain: Arc::clone(&domain),
        brain: Arc::clone(&state.brain),
        memory: state.memory.clone(),
        user_role: principal.role.as_str().to_string(),
        recovery: state.recovery_detection_config(),
        max_iterations: state.agent.max_iterations,
        workspace_mode,
        event_sinks: vec![Arc::new(sse_sink)],
        policy_registry: Some(Arc::clone(&state.policy_registry)),
        reject_high_cost: state.agent.reject_high_cost,
    })
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "Agent initialization failed");
        AppError::service_unavailable(
            "LLM provider unavailable. Run `./scripts/dev.sh health` or POST /api/models/test for the provider diagnostic.",
        )
    })?;

    // Audit hashes — `tool_schema_hash` is computed inside `build_agent`
    // at registry-build time (the only window where the tool surface is
    // reachable) and surfaced through `BuildAgentResult`. The system
    // prompt hash stamps the role-rendered prompt so a workspace admin
    // tweaking `agent_system` invalidates prior sessions.
    let system_prompt = ox_agent::build_system_prompt(&domain, principal.role.as_str()).await;
    let prompt_hash = sha256_hex(system_prompt.as_bytes());

    let audit_session_id = Uuid::new_v4();
    let audit_session = AgentSession {
        id: audit_session_id,
        user_id: user_id.clone(),
        ontology_lineage_id: req.ontology.id.clone().into(),
        prompt_hash,
        tool_schema_hash,
        model_id: model_id.clone(),
        user_message: user_message.clone(),
        final_text: None,
        created_at: Utc::now(),
        completed_at: None,
    };

    let store_for_session = Arc::clone(&state.store);
    ws_scope.spawn({
        let session = audit_session.clone();
        async move {
            if let Err(e) = store_for_session.create_agent_session(&session).await {
                tracing::warn!(error = %e, "Failed to create agent session record");
            }
        }
    });

    info!(
        user_id = %principal.id,
        audit_session_id = %audit_session_id,
        message_len = user_message.len(),
        "Agent chat stream started"
    );

    let principal_user_uuid = principal.user_uuid().ok();
    let model_id_for_stream = model_id.clone();
    let model_provider_for_stream = model_provider.clone();
    let store_for_events = Arc::clone(&state.store);
    let _stream_slot = stream_slot;
    let agent = Arc::new(agent);
    let run_budget = state.agent.run_budget.clone();

    // Drive the agent on a spawned task — every emitted event flows
    // through the fan-out sink (which includes the channel sink the
    // SSE loop reads), so the spawned task only needs to drive
    // `agent.execute` to completion. entelix's
    // `AgentEvent::Failed { envelope, .. }` carries the typed
    // [`entelix::ErrorEnvelope`] through the sink directly, so
    // `project_agent_event` projects it onto the canonical
    // `AgentEventPayload::Failed` wire shape without a side channel.
    {
        let agent = Arc::clone(&agent);
        let user_message_for_run = user_message.clone();
        let thread_id = audit_session_id.to_string();
        let run_budget = run_budget.clone();
        ws_scope.spawn(async move {
            let exec_ctx = build_execution_context(&run_budget, thread_id, ws_id);
            let initial = ReActState::from_user(user_message_for_run);
            if let Err(error) = agent.execute(initial, &exec_ctx).await {
                // The matching `AgentEvent::Failed` already flowed
                // through the SSE channel sink and was logged at
                // `warn` by `project_agent_event`. Emit only a
                // `debug` line here with the synchronous `Display`
                // form for operators tailing logs without the SSE
                // wire — duplicating the `warn` would double-count
                // every failure on alerting dashboards.
                let envelope = error.envelope();
                tracing::debug!(
                    wire_code = envelope.wire_code,
                    provider_status = envelope.provider_status,
                    error = %error,
                    "agent run terminated (synchronous return path)"
                );
            }
        });
    }

    let stream = async_stream::stream! {
        let _stream_slot = _stream_slot;

        let mut event_sequence: i32 = 0;
        let stream_deadline = tokio::time::Instant::now() + state.timeouts.chat_wall_clock;
        let mut final_text = String::new();

        loop {
            let recv = tokio::time::timeout_at(stream_deadline, sse_rx.recv()).await;
            let event = match recv {
                Ok(Some(event)) => event,
                Ok(None) => break, // Sink dropped: agent run finished.
                Err(_) => {
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
            };

            // Capture the terminal state — text + token usage — for
            // audit closure and metering. `Failed` short-circuits the
            // metering side; `final_text` stays empty so the eval
            // sampler skips below.
            if let AgentEvent::Complete { state: ref final_state, ref usage, .. } = event {
                final_text = final_state.last_assistant_text().unwrap_or_default();
                if let Some(snapshot) = usage {
                    let in_tok = i64::try_from(snapshot.input_tokens).unwrap_or(i64::MAX);
                    let out_tok = i64::try_from(snapshot.output_tokens).unwrap_or(i64::MAX);
                    let meter_store = Arc::clone(&store_for_events);
                    let meter_user_id = principal_user_uuid;
                    let meter_model = model_id_for_stream.clone();
                    let meter_provider = model_provider_for_stream.clone();
                    ws_scope.spawn(async move {
                        if let Err(error) = meter_store
                            .record_usage(
                                meter_user_id,
                                "llm",
                                Some(&meter_provider),
                                Some(&meter_model),
                                Some(operation::CHAT),
                                in_tok,
                                out_tok,
                                0,
                                0.0,
                                serde_json::json!({}),
                            )
                            .await
                        {
                            tracing::warn!(?error, "usage metering record failed");
                        }
                    });
                }
            }

            if let Some(payload) = project_agent_event(&event) {
                event_sequence += 1;
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
                ws_scope.spawn(async move {
                    if let Err(error) = store.create_agent_event(&audit_event).await {
                        tracing::warn!(?error, "agent audit event emit failed");
                    }
                });
                yield Ok(payload_to_sse(&payload));
            }
        }

        // Close out the audit session with the final assistant text.
        let store = Arc::clone(&store_for_events);
        let final_text_for_complete = final_text.clone();
        ws_scope.spawn(async move {
            if let Err(error) = store
                .complete_agent_session(audit_session_id, Some(&final_text_for_complete))
                .await
            {
                tracing::warn!(?error, %audit_session_id, "agent session completion record failed");
            }
        });

        // Online evaluation sampler — at the configured rate, drop a
        // sample (question, final answer, model) into the workspace's
        // `live_chat_samples` evaluation run.
        if !final_text.is_empty() {
            crate::eval_sampler::spawn_sample(
                Arc::clone(&store_for_events),
                ws_scope,
                crate::eval_sampler::sampling_config_from_env(),
                crate::eval_sampler::ChatSampleInput {
                    workspace_id: ws_id,
                    question: user_message.clone(),
                    answer: final_text.clone(),
                    model_id: model_id_for_stream.clone(),
                },
            );
        }
    };

    Ok(Sse::new(ws_scope.scope_stream(stream)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Build an [`LlmProviderConfig`] from the model router's resolution.
/// Returns `None` when the router did not surface a provider config —
/// the caller surfaces that as a 503 so dashboards see "model
/// unavailable" rather than a generic 500.
fn build_provider_config(
    resolved: &ox_brain::model_resolver::ResolvedModel,
    model_id: &str,
) -> Option<LlmProviderConfig> {
    let mut config = resolved.provider_config.clone()?;
    config.model = model_id.to_owned();
    Some(config)
}

fn payload_to_sse(payload: &AgentEventPayload) -> Event {
    let event_name = payload.event_type();
    let data = serde_json::to_string(payload).unwrap_or_default();
    Event::default().event(event_name).data(data)
}

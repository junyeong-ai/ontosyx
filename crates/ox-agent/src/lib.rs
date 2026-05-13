#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

//! # ox-agent
//!
//! Ontosyx agent layer — entelix-powered autonomous analysis agent.
//!
//! Wires the workspace's domain tools (knowledge graph queries,
//! ontology editing, data analysis, visualisation) into an entelix
//! ReAct agent. The build path produces an [`entelix::Agent`] with:
//!
//! - One [`entelix::tools::ToolRegistry`] carrying every domain tool
//!   the role can reach (role gating happens via
//!   `ToolRegistry::restricted_to`).
//! - A [`workspace_scope::WorkspaceScope`] layer that re-applies the
//!   four RLS task locals (`WORKSPACE_ID` / `SYSTEM_BYPASS` + the
//!   graph-runtime pair) on every tool dispatch.
//! - An [`entelix::FanOutSink`] composing [`sinks::EmbeddingSink`]
//!   (long-term memory adopt) and [`sinks::RecoveryDetectionSink`]
//!   (failure → success knowledge harvest).
//! - An optional [`entelix::RunBudget`] attached to per-execute
//!   contexts so admins cap requests / tokens / cost.

mod agent_chat;
pub mod build;
pub mod clarification_tracker;
pub mod context;
pub mod sinks;
pub mod system_prompt;
pub mod tools;
pub mod workspace_scope;

use std::sync::Arc;

use entelix::ir::{CacheControl, SystemPrompt};
use entelix::tools::{ScopedToolLayer, ToolRegistry};
use entelix::{
    AgentEventSink, ExecutionContext, FanOutSink, ReActAgentBuilder, ReActState, ToolEventLayer,
};

use ox_brain::Brain;
use ox_brain::map_entelix_err;
use ox_core::error::{OxError, OxResult};
use ox_memory::MemoryStore;

use crate::agent_chat::AgentChat;
use crate::sinks::{EmbeddingSink, RecoveryDetectionConfig, RecoveryDetectionSink};
use crate::system_prompt::tool_names_for_role;
use crate::tools::{
    ApplyOntologyTool, ConsultKnowledgeTool, EditOntologyTool, ExecuteAnalysisTool,
    ExplainOntologyTool, IntrospectSourceTool, QueryGraphTool, RecallMemoryTool,
    ResolveAmbiguityTool, SchemaEvolutionTool, SearchRecipesTool, VisualizeTool,
};
use crate::workspace_scope::WorkspaceScope;

pub use crate::build::{BuildAgentRequest, BuildAgentResult, DEFAULT_RECURSION_LIMIT};
pub use crate::context::DomainContext;
pub use crate::system_prompt::build_system_prompt;

/// Construct a fully-equipped Ontosyx agent: chat model, role-gated
/// tool registry, workspace-scope layer, fan-out sink, recursion cap.
pub async fn build_agent(config: BuildAgentRequest) -> OxResult<BuildAgentResult> {
    let domain = Arc::clone(&config.domain);
    let brain = Arc::clone(&config.brain);

    // 1. Tool registry — register every tool the role can reach. The
    //    registry is in hand at the same point we need (a) the
    //    audit-grade tool-surface fingerprint
    //    (`canonical_fingerprint` — SHA-256 over the lexically-sorted
    //    `{name, description, input_schema, output_schema}` payload,
    //    patch-version-stable upstream) and (b) the model-facing
    //    [`entelix::ir::ToolSpec`] slice the LLM sees on every
    //    dispatch.
    let registry = build_role_registry(&domain, &brain, &config).await?;
    let tool_schema_hash = registry.canonical_fingerprint();
    let tool_specs = registry.tool_specs();

    // 2. Chat handle — bake the agent's persona (system prompt) and
    //    advertised tool surface into one
    //    `Runnable<Vec<Message>, Message>`. Without the tools on the
    //    request the model has no way to know it can call them;
    //    without the system prompt the agent has no persona. Both are
    //    cached at the codec edge (`CacheControl::one_hour`) so the
    //    prefix re-uses across every turn of one run.
    let system_text = build_system_prompt(&domain, &config.user_role).await;
    let system_prompt = SystemPrompt::cached(system_text, CacheControl::one_hour());
    let chat = config
        .chat_model_registry
        .get_or_build(&config.provider_config)
        .await?;
    let chat = AgentChat::new(chat, system_prompt, tool_specs);

    // 3. Sink — fan-out of the domain sinks (`EmbeddingSink`,
    //    `RecoveryDetectionSink`) plus any caller-supplied event sink
    //    (typically a per-request channel sink the HTTP route
    //    forwards to its SSE wire). Observe-only contract: a sink
    //    returning `Err` would halt the agent, so the fan-out
    //    swallows internal failures via `tracing::warn!` and the
    //    domain sinks return `Ok(())` for every event.
    let sink = build_fan_out_sink(&domain, &config.memory, config.recovery, config.event_sinks);

    // 4. Tool registry layers — `ToolEventLayer` registered first →
    //    innermost, `ScopedToolLayer` registered last → outermost.
    //    The `ToolEventLayer.call` body emits sink events both before
    //    and after the inner dispatch await; only the await itself
    //    sits inside the scope `ScopedToolLayer` sets, so wrapping
    //    the entire `ToolEventLayer` (sink emissions included) under
    //    the workspace scope is the only way to keep RLS task-locals
    //    visible to the sink — embedding / recovery sinks
    //    `tokio::spawn` workspace-scoped store writes that would
    //    otherwise hit the RLS deny-all branch.
    let mut registry = registry
        .layer(ToolEventLayer::<ReActState>::new(Arc::clone(&sink)))
        .layer(ScopedToolLayer::new(WorkspaceScope::new(
            config.workspace_mode,
        )));
    if let Some(policy) = config.policy_registry.as_ref() {
        registry = registry.layer(entelix::PolicyLayer::new(Arc::clone(policy)));
    }

    // 5. Recursion cap — graph-level step ceiling. `RunBudget` rides
    //    on the per-execute [`ExecutionContext`], constructed by the
    //    HTTP route via [`build_execution_context`].
    let recursion_limit = if config.max_iterations == 0 {
        DEFAULT_RECURSION_LIMIT
    } else {
        config.max_iterations
    };

    let agent = ReActAgentBuilder::new(chat, registry)
        .with_recursion_limit(recursion_limit as usize)
        .add_sink(sink)
        .build()
        .map_err(|e| OxError::Runtime {
            message: format!("Agent build failed: {e}"),
        })?;

    Ok(BuildAgentResult {
        agent,
        tool_schema_hash,
    })
}

/// Build a per-execute [`ExecutionContext`] for `agent.execute*`.
///
/// Mints a fresh [`entelix::RunBudget`] from `run_budget` (counters
/// reset every run), stamps the thread id so multi-turn audit /
/// persistence joins on the same conversation, and binds the
/// workspace as the entelix [`entelix::TenantId`] so the
/// `PolicyLayer`'s per-tenant ledger keys off the workspace boundary
/// instead of the shared `default` tenant. The caller layers any
/// additional extensions on top — typically
/// `add_extension(ProgressReporter::new(sse_sink))` so brain-level
/// progress events ride into the SSE wire.
#[must_use]
pub fn build_execution_context(
    run_budget: &ox_brain::RunBudgetCaps,
    thread_id: impl Into<String>,
    workspace_id: uuid::Uuid,
) -> ExecutionContext {
    ExecutionContext::default()
        .with_thread_id(thread_id)
        .with_run_budget(run_budget.build())
        .with_tenant_id(entelix::TenantId::new(workspace_id.to_string()))
}

/// Build the role-gated tool registry. Designer / admin roles see
/// every mutating tool; viewers see the read-only subset.
async fn build_role_registry(
    domain: &Arc<DomainContext>,
    brain: &Arc<dyn Brain>,
    config: &BuildAgentRequest,
) -> OxResult<ToolRegistry<()>> {
    use entelix::SchemaToolExt;
    let mut registry = ToolRegistry::<()>::new()
        .register(Arc::new(
            QueryGraphTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
                reject_high_cost: config.reject_high_cost,
            }
            .into_adapter(),
        ))
        .map_err(map_entelix_err("tool registry build failed"))?
        .register(Arc::new(
            EditOntologyTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
            }
            .into_adapter(),
        ))
        .map_err(map_entelix_err("tool registry build failed"))?
        .register(Arc::new(
            ExecuteAnalysisTool {
                store: Arc::clone(&domain.store) as Arc<dyn ox_store::RecipeExecutionStore>,
            }
            .into_adapter(),
        ))
        .map_err(map_entelix_err("tool registry build failed"))?
        .register(Arc::new(
            ExplainOntologyTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
            }
            .into_adapter(),
        ))
        .map_err(map_entelix_err("tool registry build failed"))?
        .register(Arc::new(VisualizeTool.into_adapter()))
        .map_err(map_entelix_err("tool registry build failed"))?
        .register(Arc::new(
            SearchRecipesTool {
                store: Arc::clone(&domain.store),
            }
            .into_adapter(),
        ))
        .map_err(map_entelix_err("tool registry build failed"))?;

    if domain.ontology_draft_id.is_some() && domain.ontology.is_some() {
        registry = registry
            .register(Arc::new(
                ApplyOntologyTool {
                    domain: Arc::clone(domain),
                    brain: Arc::clone(brain),
                }
                .into_adapter(),
            ))
            .map_err(map_entelix_err("tool registry build failed"))?;
    }

    if let Some(mem) = &config.memory {
        registry = registry
            .register(Arc::new(
                RecallMemoryTool {
                    memory: Arc::clone(mem),
                    // Pass the lineage (OntologyIR.id), not the saved-row UUID.
                    // Memory entries are filtered by `ontology_lineage_id` so a
                    // UUID-shaped string would never match.
                    ontology_lineage_id: domain.current_ontology().map(|o| o.id.clone()),
                }
                .into_adapter(),
            ))
            .map_err(map_entelix_err("tool registry build failed"))?;
    }

    if domain.source_schema.is_some() {
        registry = registry
            .register(Arc::new(
                IntrospectSourceTool {
                    domain: Arc::clone(domain),
                }
                .into_adapter(),
            ))
            .map_err(map_entelix_err("tool registry build failed"))?;
    }

    if domain.source_schema.is_some() && domain.ontology.is_some() {
        registry = registry
            .register(Arc::new(
                SchemaEvolutionTool {
                    domain: Arc::clone(domain),
                }
                .into_adapter(),
            ))
            .map_err(map_entelix_err("tool registry build failed"))?;
    }

    let current_ontology_at_build = domain.current_ontology();
    if let Some(kb) = &domain.knowledge_store {
        registry = registry
            .register(Arc::new(
                ConsultKnowledgeTool {
                    knowledge_store: Arc::clone(kb),
                    ontology_name: current_ontology_at_build.as_ref().map(|o| o.name.clone()),
                    ontology_version: current_ontology_at_build
                        .as_ref()
                        .map(|o| o.version.number as i32),
                }
                .into_adapter(),
            ))
            .map_err(map_entelix_err("tool registry build failed"))?;
    }

    if let Some(ambig) = &domain.ambiguity_store {
        registry = registry
            .register(Arc::new(
                ResolveAmbiguityTool {
                    ambiguity_store: Arc::clone(ambig),
                    clarification_tracker: Arc::clone(&domain.clarification_tracker),
                }
                .into_adapter(),
            ))
            .map_err(map_entelix_err("tool registry build failed"))?;
    }

    let allowed = tool_names_for_role(&config.user_role);
    let allowed_refs: Vec<&str> = allowed
        .iter()
        .copied()
        .filter(|name| registry.names().any(|registered| registered == *name))
        .collect();

    let restricted = registry
        .restricted_to(&allowed_refs)
        .map_err(map_entelix_err("tool registry build failed"))?;
    Ok(restricted)
}

/// Build the fan-out [`AgentEventSink`] that runs alongside the
/// agent — `EmbeddingSink` (long-term memory adopt),
/// `RecoveryDetectionSink` (failure → success knowledge harvest),
/// plus any caller-supplied sinks (typically a per-request channel
/// sink that forwards events to an SSE wire).
fn build_fan_out_sink(
    domain: &Arc<DomainContext>,
    memory: &Option<Arc<MemoryStore>>,
    recovery: RecoveryDetectionConfig,
    extra: Vec<Arc<dyn AgentEventSink<ReActState>>>,
) -> Arc<dyn AgentEventSink<ReActState>> {
    let mut fan_out = FanOutSink::<ReActState>::new();

    if let Some(mem) = memory {
        let ontology_lineage_id = domain.current_ontology().map(|o| o.id.clone());
        let retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>> =
            Some(Arc::clone(&domain.store) as Arc<dyn ox_store::EmbeddingRetryStore>);
        fan_out = fan_out.push(Arc::new(EmbeddingSink::with_ontology_lineage_id(
            Arc::clone(mem),
            ontology_lineage_id,
            retry_store,
        )));
    }

    if let (Some(kb), Some(ontology)) = (&domain.knowledge_store, domain.current_ontology()) {
        fan_out = fan_out.push(Arc::new(RecoveryDetectionSink::new(
            Arc::clone(kb),
            memory.clone(),
            domain.workspace_id,
            ontology.name.clone(),
            ontology.version.number as i32,
            recovery,
        )));
    }

    for sink in extra {
        fan_out = fan_out.push(sink);
    }

    Arc::new(fan_out)
}

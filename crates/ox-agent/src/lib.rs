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
//! Ontosyx agent layer — branchforge-powered autonomous analysis agent.
//!
//! Provides domain-specific tools for knowledge graph querying, ontology editing,
//! data analysis, and visualization. Built on the branchforge agent runtime
//! for durable sessions, tool execution, and human-in-the-loop workflows.

pub mod clarification_tracker;
pub mod hooks;
pub mod recipes;
pub mod tools;

use std::sync::Arc;

use arc_swap::ArcSwap;
use branchforge::{Agent, Auth, CacheConfig, ExecutionMode, ToolSurface};
use hooks::{EmbeddingHook, RecoveryDetectionHook, RecoveryHookConfig};
use ox_compiler::GraphCompiler;
use ox_core::error::OxResult;
use ox_ontology::ir::OntologyIR;
use ox_memory::MemoryStore;
use ox_runtime::GraphRuntime;
use ox_store::Store;
use tools::{
    ApplyOntologyTool, ConsultKnowledgeTool, EditOntologyTool, ExecuteAnalysisTool,
    ExplainOntologyTool, IntrospectSourceTool, QueryGraphTool, RecallMemoryTool,
    ResolveAmbiguityTool, SchemaEvolutionTool, SearchRecipesTool, VisualizeTool,
};

// Agent system prompt is loaded from DB (prompt_templates, name="agent_system").
// Seeded from prompts/agent_system.toml on first run.

// ---------------------------------------------------------------------------
// DomainContext — shared state for all agent tools
// ---------------------------------------------------------------------------

/// Shared state for all agent tools — graph backends, store, and current ontology context.
///
/// `ontology` is an `ArcSwap` (not a plain `Arc`) so tools that mutate
/// the ontology — `apply_ontology`, `edit_ontology` — can publish the
/// new snapshot into the DomainContext without needing to rebuild it.
/// Downstream tools in the same session read the latest snapshot right
/// before wrapping `runtime.execute_query` in `GRAPH_ONTOLOGY.scope`,
/// so schema edits take effect on the very next query.
pub struct DomainContext {
    pub compiler: Arc<dyn GraphCompiler>,
    pub runtime: Option<Arc<dyn GraphRuntime>>,
    pub store: Arc<dyn Store>,
    pub ontology: Option<ArcSwap<OntologyIR>>,
    pub user_id: String,
    pub workspace_id: uuid::Uuid,
    /// Identity of the ontology this session is pinned to (matches
    /// `ontologies.id`). `None` for ad-hoc sessions operating on a draft
    /// IR that has not been committed through `OntologyVersionStore` yet.
    pub ontology_id: Option<uuid::Uuid>,
    pub project_id: Option<uuid::Uuid>,
    pub project_revision: Option<i32>,
    /// Source schema for introspection (available when project has been analyzed).
    pub source_schema: Option<ox_core::source_schema::SourceSchema>,
    /// Source profile (column statistics) for introspection.
    pub source_profile: Option<ox_core::source_schema::SourceProfile>,
    /// Repo analysis summary (framework, domain notes, field hints) from project creation.
    pub repo_insights: Option<ox_ontology::repo_insights::RepoInsights>,
    /// Knowledge store for failure-driven learning corrections.
    pub knowledge_store: Option<Arc<dyn ox_store::KnowledgeStore>>,
    /// Ambiguity resolver store. Wired when the session has access to a
    /// source — the `resolve_ambiguity` tool is registered only when
    /// this is populated, so ad-hoc sessions without a source surface
    /// aren't offered a tool that has nothing to resolve against.
    pub ambiguity_store: Option<Arc<dyn ox_store::AmbiguityStore>>,
    /// Per-agent-process "session has resolved an ambiguity recently"
    /// tracker. Populated by `ResolveAmbiguityTool` and read by
    /// `QueryGraphTool` so the `clarification_success_rate` quality
    /// signal can flip `ambiguity_was_clarified = true` on a query
    /// that followed a clarification in the same session.
    pub clarification_tracker: clarification_tracker::SharedClarificationTracker,
    /// Original user question — always passed to translate_query as primary context.
    /// Prevents agent-driven question fragmentation that defeats graph traversal.
    pub user_question: Option<String>,
}

impl DomainContext {
    /// Load the current ontology snapshot. Returns `None` when no
    /// ontology has been attached to this session. Callers that need
    /// a short-lived reference should hold the `Arc` across a single
    /// tool invocation rather than for the entire session so a
    /// mid-session edit can publish a replacement.
    pub fn current_ontology(&self) -> Option<Arc<OntologyIR>> {
        self.ontology.as_ref().map(|o| o.load_full())
    }

    /// Publish a replacement ontology. Called by tools that mutate the
    /// ontology (e.g. apply_ontology, edit_ontology) so every
    /// subsequent tool in the session sees the new snapshot.
    ///
    /// Returns `true` when a replacement was stored, `false` when the
    /// session has no ontology slot (and therefore no subscribers).
    pub fn replace_ontology(&self, ontology: OntologyIR) -> bool {
        match &self.ontology {
            Some(slot) => {
                slot.store(Arc::new(ontology));
                true
            }
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// OntosyxAgentConfig — agent construction parameters
// ---------------------------------------------------------------------------

/// Parameters for constructing a branchforge agent with Ontosyx domain tools.
pub struct OntosyxAgentConfig {
    pub auth: Auth,
    pub model: String,
    pub execution_mode: ExecutionMode,
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn ox_brain::Brain>,
    pub memory: Option<Arc<MemoryStore>>,
    pub session_id: Option<String>,
    /// User role for tool access control: "admin", "designer", "viewer".
    pub user_role: String,
    /// Runtime thresholds for the `RecoveryDetectionHook`. Pass
    /// `RecoveryHookConfig::default()` for the previous behavior.
    pub recovery: RecoveryHookConfig,
    /// Upper bound on planner iterations (LLM turn + tool call).
    /// 0 falls back to a sensible built-in default so older callers
    /// don't need to know about the ceiling.
    #[allow(clippy::struct_field_names)]
    pub max_iterations: u32,
    /// Reject `RiskLevel::High` queries in `QueryGraphTool` before they
    /// reach the driver. Applied uniformly across tools; the value
    /// is injected here rather than read via another task-local so
    /// a headless test harness can vary it without touching globals.
    pub reject_high_cost: bool,
}

/// Built-in ceiling used when the caller passes `max_iterations = 0`.
/// Matches the previous hard-coded value so no caller sees a behavior
/// change until they opt into a different budget.
pub const DEFAULT_MAX_ITERATIONS: u32 = 16;

// ---------------------------------------------------------------------------
// build_agent — construct a fully-equipped Ontosyx agent
// ---------------------------------------------------------------------------

/// Result of `build_agent` — includes metadata about session resume status
/// so the caller can emit appropriate client events.
pub struct BuildAgentResult {
    pub agent: Agent,
    /// `true` when an existing session was successfully resumed.
    /// `false` when no session_id was provided, or resume failed and a
    /// fresh session was created instead.
    pub session_resumed: bool,
}

/// Construct a fully-equipped Ontosyx agent, optionally resuming an existing session.
pub async fn build_agent(config: OntosyxAgentConfig) -> OxResult<BuildAgentResult> {
    let domain = config.domain;
    let brain = config.brain;
    let system_prompt = build_system_prompt(&domain, &config.user_role).await;

    /// Configure an AgentBuilder with all domain tools, hooks, and settings.
    async fn configure_builder(
        auth: Auth,
        model: &str,
        user_role: &str,
        system_prompt: &str,
        execution_mode: ExecutionMode,
        max_iterations: u32,
        reject_high_cost: bool,
        domain: &Arc<DomainContext>,
        brain: &Arc<dyn ox_brain::Brain>,
        memory: &Option<Arc<MemoryStore>>,
        recovery_cfg: RecoveryHookConfig,
    ) -> OxResult<branchforge::AgentBuilder> {
        let mut builder = Agent::builder()
            .auth(auth)
            .await
            .map_err(|e| ox_core::error::OxError::Runtime {
                message: format!("Agent auth failed: {e}"),
            })?
            .model(model)
            .tools(tool_surface_for_role(user_role))
            .tool(QueryGraphTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
                reject_high_cost,
            })
            .tool(EditOntologyTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
            });

        // Apply ontology tool requires a project context to save changes
        if domain.project_id.is_some() && domain.ontology.is_some() {
            builder = builder.tool(ApplyOntologyTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
            });
        }

        builder = builder
            .tool(ExecuteAnalysisTool {
                store: Arc::clone(&domain.store) as Arc<dyn ox_store::AnalysisResultStore>,
            })
            .tool(ExplainOntologyTool {
                domain: Arc::clone(domain),
                brain: Arc::clone(brain),
            })
            .tool(VisualizeTool)
            .system_prompt(system_prompt.to_owned())
            .execution_mode(execution_mode)
            .max_iterations(max_iterations as usize)
            .cache(CacheConfig::static_and_tools());

        // RAG tools
        if let Some(mem) = memory {
            builder = builder.tool(RecallMemoryTool {
                memory: Arc::clone(mem),
                // Pass the lineage (OntologyIR.id), not the saved-row UUID.
                // Memory entries are filtered by `ontology_lineage_id` so a
                // UUID-shaped string would never match.
                ontology_lineage_id: domain.current_ontology().map(|o| o.id.clone()),
            });
        }
        builder = builder.tool(SearchRecipesTool {
            store: Arc::clone(&domain.store),
        });

        // Source introspection tool (progressive disclosure for large schemas)
        if domain.source_schema.is_some() {
            builder = builder.tool(IntrospectSourceTool {
                domain: Arc::clone(domain),
            });
        }

        // Schema evolution tool (requires both source schema and ontology)
        if domain.source_schema.is_some() && domain.ontology.is_some() {
            builder = builder.tool(SchemaEvolutionTool {
                domain: Arc::clone(domain),
            });
        }

        // Knowledge base tool (requires knowledge store + ontology context)
        //
        // Reads are taken at agent-build time. If the user later edits
        // the ontology mid-session, the knowledge tool sees the
        // *construction-time* name + version; that's the correct
        // behavior — knowledge entries are version-scoped, and a
        // post-edit build step re-creates the agent for a fresh
        // session.
        let current_ontology_at_build = domain.current_ontology();
        if let Some(kb) = &domain.knowledge_store {
            builder = builder.tool(ConsultKnowledgeTool {
                knowledge_store: Arc::clone(kb),
                ontology_name: current_ontology_at_build.as_ref().map(|o| o.name.clone()),
                ontology_version: current_ontology_at_build
                    .as_ref()
                    .map(|o| o.version.number as i32),
            });
        }

        // Ambiguity resolver tool. Only meaningful when an
        // `AmbiguityStore` has been threaded through — ad-hoc ontology
        // sessions without a source surface register no resolver, so
        // the agent doesn't see a tool that has nothing to resolve.
        if let Some(ambig) = &domain.ambiguity_store {
            builder = builder.tool(ResolveAmbiguityTool {
                ambiguity_store: Arc::clone(ambig),
                clarification_tracker: Arc::clone(&domain.clarification_tracker),
            });
        }

        // Embedding hook for long-term memory
        if let Some(mem) = memory {
            // Embed content with the lineage so later RAG filters hit it.
            // Historical note: an earlier refactor passed the ontology
            // identity UUID here; it never matched the lineage-string
            // field the RAG filter compared against. Keep lineage_id as
            // the canonical embedding scope.
            let ontology_lineage_id = current_ontology_at_build.as_ref().map(|o| o.id.clone());
            let retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>> =
                Some(Arc::clone(&domain.store) as Arc<dyn ox_store::EmbeddingRetryStore>);
            builder = builder.hook(EmbeddingHook::with_ontology_lineage_id(
                Arc::clone(mem),
                ontology_lineage_id,
                retry_store,
            ));
        }

        // Recovery detection hook: auto-creates knowledge when query_graph
        // fails then succeeds in the same session.
        if let Some(kb) = &domain.knowledge_store
            && let Some(ontology) = current_ontology_at_build.as_ref()
        {
            builder = builder.hook(RecoveryDetectionHook::new(
                Arc::clone(kb),
                memory.clone(),
                domain.workspace_id,
                ontology.name.clone(),
                ontology.version.number as i32,
                recovery_cfg,
            ));
        }

        Ok(builder)
    }

    let max_iterations = if config.max_iterations == 0 {
        DEFAULT_MAX_ITERATIONS
    } else {
        config.max_iterations
    };
    let mut builder = configure_builder(
        config.auth.clone(),
        &config.model,
        &config.user_role,
        &system_prompt,
        config.execution_mode.clone(),
        max_iterations,
        config.reject_high_cost,
        &domain,
        &brain,
        &config.memory,
        config.recovery,
    )
    .await?;

    // Resume existing session for multi-turn conversation.
    // If resume fails (e.g. stale session after server restart), rebuild fresh
    // and signal the caller so it can notify the client.
    let mut session_resumed = false;
    if let Some(session_id) = config.session_id {
        match builder.resume_session(session_id.clone()).await {
            Ok(resumed) => {
                builder = resumed;
                session_resumed = true;
                tracing::info!(session_id = %session_id, "Resumed existing session");
            }
            Err(e) => {
                tracing::warn!(session_id = %session_id, error = %e, "Session resume failed — starting fresh");
                builder = configure_builder(
                    config.auth,
                    &config.model,
                    &config.user_role,
                    &system_prompt,
                    config.execution_mode,
                    max_iterations,
                    config.reject_high_cost,
                    &domain,
                    &brain,
                    &config.memory,
                    config.recovery,
                )
                .await?;
            }
        }
    }

    let agent = builder
        .build()
        .await
        .map_err(|e| ox_core::error::OxError::Runtime {
            message: format!("Agent build failed: {e}"),
        })?;

    Ok(BuildAgentResult {
        agent,
        session_resumed,
    })
}

/// Public accessor for the system prompt text — used for hash computation
/// in the chat handler for audit/replay.
pub async fn system_prompt_text(domain: &DomainContext, user_role: &str) -> String {
    build_system_prompt(domain, user_role).await
}

/// Build the system prompt.
///
/// Loads the base prompt from DB (prompt_templates, name="agent_system").
/// Appends role and ontology context (deterministic, cacheable).
async fn build_system_prompt(domain: &DomainContext, user_role: &str) -> String {
    // Workspace-scoped lookup: prefer this workspace's override, fall
    // back to the global template. A workspace admin can override the
    // base agent_system prompt without affecting other tenants.
    let lookup = domain
        .store
        .get_active_prompt_for_workspace("agent_system", Some(domain.workspace_id))
        .await;
    let base = match lookup {
        Ok(Some(row)) => row.content,
        Ok(None) => {
            tracing::error!("agent_system prompt missing from DB — using minimal fallback");
            "You are Ontosyx, a knowledge graph assistant.".to_string()
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to load agent_system prompt — using minimal fallback");
            "You are Ontosyx, a knowledge graph assistant.".to_string()
        }
    };

    let mut prompt = base;

    // User role context
    match user_role {
        "viewer" => {
            prompt.push_str(
                "\nThe current user has **viewer** role. \
                 You can query and explain data, but cannot modify the ontology or execute analyses.\n",
            );
        }
        "designer" => {
            prompt.push_str(
                "\nThe current user has **designer** role. \
                 You have full access to all tools.\n",
            );
        }
        "admin" => {
            prompt.push_str(
                "\nThe current user has **admin** role. \
                 You have full access to all tools and system configuration.\n",
            );
        }
        _ => {}
    }

    // Ontology context
    if let Some(ontology) = domain.current_ontology() {
        prompt.push_str(&format!(
            "\nCurrent ontology: '{}' (v{})\n\
             Node types: {}\n\
             Edge types: {}\n",
            ontology.name,
            ontology.version.number,
            ontology
                .node_types()
                .iter()
                .map(|n| n.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            ontology
                .edge_types()
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    // Source code insights from repo analysis (framework, domain notes, field hints)
    if let Some(insights) = &domain.repo_insights {
        prompt.push_str("\n\n--- Source Code Insights ---\n");
        if let Ok(formatted) = serde_json::to_string_pretty(insights) {
            prompt.push_str(&formatted);
        }
    }

    // Knowledge base: learned corrections and admin hints
    if let (Some(kb), Some(ontology)) = (&domain.knowledge_store, domain.current_ontology()) {
        match kb
            .list_active_knowledge(
                &ontology.name,
                ontology.version.number as i32,
                &["correction", "hint"],
                10,
            )
            .await
        {
            Ok(entries) if !entries.is_empty() => {
                prompt.push_str("\n\n--- Learned Knowledge ---\n");
                for e in &entries {
                    prompt.push_str(&format!("- [{}] {}\n", e.kind, e.content));
                }
            }
            _ => {}
        }
    }

    prompt
}

/// Determine which tools are available based on user role.
///
/// - **viewer**: Read-only tools (query, explain, visualize)
/// - **designer/admin**: Full tool set including edit and analysis
fn tool_surface_for_role(role: &str) -> ToolSurface {
    match role {
        "viewer" => ToolSurface::only([
            tools::QUERY_GRAPH,
            tools::EXPLAIN_ONTOLOGY,
            tools::VISUALIZE,
            tools::RECALL_MEMORY,
            tools::SEARCH_RECIPES,
            tools::INTROSPECT_SOURCE,
            tools::CONSULT_KNOWLEDGE,
        ]),
        _ => ToolSurface::only([
            tools::QUERY_GRAPH,
            tools::EDIT_ONTOLOGY,
            tools::APPLY_ONTOLOGY,
            tools::EXECUTE_ANALYSIS,
            tools::EXPLAIN_ONTOLOGY,
            tools::VISUALIZE,
            tools::RECALL_MEMORY,
            tools::SEARCH_RECIPES,
            tools::INTROSPECT_SOURCE,
            tools::SCHEMA_EVOLUTION,
            tools::CONSULT_KNOWLEDGE,
        ]),
    }
}

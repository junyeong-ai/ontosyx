//! # ox-agent
//!
//! Ontosyx agent layer — branchforge-powered autonomous analysis agent.
//!
//! Provides domain-specific tools for knowledge graph querying, ontology editing,
//! data analysis, and visualization. Built on the branchforge agent runtime
//! for durable sessions, tool execution, and human-in-the-loop workflows.

pub mod hooks;
pub mod recipes;
pub mod tools;

use std::sync::Arc;

use branchforge::{Agent, Auth, CacheConfig, ExecutionMode, ToolSurface};
use hooks::EmbeddingHook;
use ox_compiler::GraphCompiler;
use ox_core::error::OxResult;
use ox_core::ontology_ir::OntologyIR;
use ox_memory::MemoryStore;
use ox_runtime::GraphRuntime;
use ox_store::Store;
use tools::{
    ApplyOntologyTool, EditOntologyTool, ExecuteAnalysisTool, ExplainOntologyTool,
    IntrospectSourceTool, QueryGraphTool, RecallMemoryTool, SchemaEvolutionTool, SearchRecipesTool,
    VisualizeTool,
};

// Agent system prompt is loaded from DB (prompt_templates, name="agent_system").
// Seeded from prompts/agent_system.toml on first run.

// ---------------------------------------------------------------------------
// DomainContext — shared state for all agent tools
// ---------------------------------------------------------------------------

/// Shared state for all agent tools — graph backends, store, and current ontology context.
pub struct DomainContext {
    pub compiler: Arc<dyn GraphCompiler>,
    pub runtime: Option<Arc<dyn GraphRuntime>>,
    pub store: Arc<dyn Store>,
    pub ontology: Option<OntologyIR>,
    pub user_id: String,
    pub saved_ontology_id: Option<uuid::Uuid>,
    pub project_id: Option<uuid::Uuid>,
    pub project_revision: Option<i32>,
    /// Source schema for introspection (available when project has been analyzed).
    pub source_schema: Option<ox_core::source_schema::SourceSchema>,
    /// Source profile (column statistics) for introspection.
    pub source_profile: Option<ox_core::source_schema::SourceProfile>,
    /// Repo analysis summary (framework, domain notes, field hints) from project creation.
    pub repo_insights: Option<ox_core::repo_insights::RepoInsights>,
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
}

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
        domain: &Arc<DomainContext>,
        brain: &Arc<dyn ox_brain::Brain>,
        memory: &Option<Arc<MemoryStore>>,
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
            .cache(CacheConfig::static_and_tools());

        // RAG tools
        if let Some(mem) = memory {
            builder = builder.tool(RecallMemoryTool {
                memory: Arc::clone(mem),
                ontology_id: domain.saved_ontology_id.map(|id| id.to_string()),
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

        // Embedding hook for long-term memory
        if let Some(mem) = memory {
            let ontology_id = domain.saved_ontology_id.map(|id| id.to_string());
            let retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>> =
                Some(Arc::clone(&domain.store) as Arc<dyn ox_store::EmbeddingRetryStore>);
            builder = builder.hook(EmbeddingHook::with_ontology_id(
                Arc::clone(mem),
                ontology_id,
                retry_store,
            ));
        }

        Ok(builder)
    }

    let mut builder = configure_builder(
        config.auth.clone(),
        &config.model,
        &config.user_role,
        &system_prompt,
        config.execution_mode.clone(),
        &domain,
        &brain,
        &config.memory,
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
                    &domain,
                    &brain,
                    &config.memory,
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
    let base = match domain.store.get_active_prompt("agent_system").await {
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
    if let Some(ontology) = &domain.ontology {
        prompt.push_str(&format!(
            "\nCurrent ontology: '{}' (v{})\n\
             Node types: {}\n\
             Edge types: {}\n",
            ontology.name,
            ontology.version,
            ontology
                .node_types
                .iter()
                .map(|n| n.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            ontology
                .edge_types
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
        ]),
    }
}

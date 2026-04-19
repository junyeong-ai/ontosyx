use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::RwLock;

use branchforge::Auth;
use ox_brain::Brain;
use ox_brain::client_pool::ClientPool;
use ox_compiler::{GraphCompiler, PlanCacheHandle};
use ox_runtime::GraphRuntime;
use ox_source::registry::AdapterRegistry;
use ox_store::{Store, ToolApproval};

use crate::model_router::DbModelRouter;

use crate::collaboration::CollaborationHub;
use crate::config::{AgentConfig, AuthConfig, DashboardsConfig, RecoveryConfig, TimeoutsConfig};
use crate::middleware::RateLimiter;
use crate::sso::OidcProviderRegistry;
use crate::system_config::SystemConfig;

/// Repo enrichment security policy.
#[derive(Clone, Default)]
pub struct RepoPolicy {
    pub allowed_roots: Vec<String>,
    pub allowed_git_hosts: Vec<String>,
}

/// Application state shared across all request handlers.
#[derive(Clone)]
pub struct AppState {
    pub brain: Arc<dyn Brain>,
    pub compiler: Arc<dyn GraphCompiler>,
    /// `Some` when `compiler` is a `PlanCache`; exposes stats + invalidation
    /// without forcing callers to know the inner compiler type.
    pub plan_cache: Option<Arc<dyn PlanCacheHandle>>,
    pub runtime: Option<Arc<dyn GraphRuntime>>,
    /// Optional read-only graph runtime used by the MCP `execute_cypher`
    /// tool. When configured, MCP raw Cypher executes under credentials
    /// that physically cannot mutate the graph (defense-in-depth on top
    /// of the keyword heuristic in `mcp::forbidden_cypher_keyword`).
    pub readonly_runtime: Option<Arc<dyn GraphRuntime>>,
    pub store: Arc<dyn Store>,
    pub timeouts: Timeouts,
    pub auth_config: AuthConfig,
    pub repo_policy: RepoPolicy,
    pub adapter_registry: Arc<AdapterRegistry>,
    pub system_config: Arc<RwLock<SystemConfig>>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
    pub memory: Option<Arc<ox_memory::MemoryStore>>,
    pub client_pool: Arc<ClientPool>,
    pub model_router: Arc<DbModelRouter>,
    pub agent_auth: Auth,
    /// Generic OIDC provider registry (Google, Microsoft, Okta, etc.)
    pub oidc_providers: Arc<OidcProviderRegistry>,
    /// HITL: maps "session_id:tool_call_id" → oneshot sender for tool approval
    pub tool_review_channels:
        Option<Arc<DashMap<String, tokio::sync::oneshot::Sender<ToolApproval>>>>,
    /// Real-time collaboration hub (presence, cursors, locks)
    #[allow(dead_code)] // Awaiting WebSocket route integration
    pub collaboration: Arc<CollaborationHub>,
    /// Dashboard share-token configuration (default + max expiry).
    pub dashboards: DashboardsConfig,
    /// Recovery-detection hook tuning.
    pub recovery: RecoveryConfig,
    /// Agent-loop budgets (`max_iterations`, future: token cap, cost cap).
    pub agent: AgentConfig,
    /// Per-user concurrent chat-stream limiter (defense-in-depth alongside
    /// the global request rate limiter).
    pub stream_limiter: Arc<crate::stream_limiter::StreamLimiter>,
}

impl AppState {
    /// Pluck the recovery thresholds in the form the agent crate
    /// expects, without making ox-agent depend on ox-api's `OxConfig`.
    pub fn recovery_hook_config(&self) -> ox_agent::hooks::RecoveryHookConfig {
        ox_agent::hooks::RecoveryHookConfig {
            jaccard_threshold: self.recovery.jaccard_threshold,
            session_window_minutes: self.recovery.session_window_minutes,
        }
    }
}

/// Pre-computed Duration values from config.
///
/// Profiling and refine timeouts are now runtime-tunable via `SystemConfig`
/// and read directly from there in each handler.
#[derive(Clone)]
pub struct Timeouts {
    pub design_operation: Duration,
    pub raw_query: Duration,
    pub health_check: Duration,
    pub analysis: Duration,
    /// Wall-clock ceiling on a single chat-stream agent loop. Applied at
    /// route level via `tokio::time::timeout`.
    pub chat_wall_clock: Duration,
}

impl From<&TimeoutsConfig> for Timeouts {
    fn from(config: &TimeoutsConfig) -> Self {
        Self {
            design_operation: Duration::from_secs(config.design_operation_secs),
            raw_query: Duration::from_secs(config.raw_query_secs),
            health_check: Duration::from_secs(config.health_check_secs),
            analysis: Duration::from_secs(config.analysis_secs),
            chat_wall_clock: Duration::from_secs(config.chat_wall_clock_secs),
        }
    }
}

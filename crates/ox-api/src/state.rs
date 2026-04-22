use std::sync::Arc;
use std::time::Duration;

use axum::extract::FromRef;
use dashmap::DashMap;
use tokio::sync::{OnceCell, RwLock};

use branchforge::Auth;
use ox_brain::Brain;
use ox_brain::client_pool::ClientPool;
use ox_compiler::{GraphCompiler, PlanCacheHandle};
use ox_federation::InMemoryAdapterResolver;
use ox_runtime::GraphRuntime;
use ox_source::registry::AdapterRegistry;
use ox_store::{Store, ToolApproval};
use uuid::Uuid;

use crate::model_router::DbModelRouter;

use crate::collaboration::CollaborationHub;
use crate::config::{AgentConfig, AuthConfig, DashboardsConfig, RecoveryConfig, TimeoutsConfig};
use crate::credential::SecretResolver;
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
    /// Per-workspace runtime resolvers. See [`WorkspaceResolverSlot`]
    /// for the per-workspace slot shape.
    pub federation_resolvers: Arc<DashMap<Uuid, Arc<WorkspaceResolverSlot>>>,
    /// Dereferences `Credential::SecretRef { value: "env:X" }` (and
    /// any future `vault:` / `aws-sm:` schemes) to concrete secret
    /// values at adapter-build time. Kept as a trait object so the
    /// prod server wires `EnvSecretResolver` while tests can inject
    /// a deterministic fake.
    pub secret_resolver: Arc<dyn SecretResolver>,
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
    /// Process-wide "has this session resolved an ambiguity recently?"
    /// tracker. Lives on `AppState` because a chat session spans
    /// multiple independent chat-stream requests; a per-request
    /// `DomainContext` field would reset the timestamp between a
    /// `resolve_ambiguity` call and the follow-up `query_graph`
    /// call. Feeds the Phase 4.6 `clarification_success_rate`
    /// quality signal.
    pub clarification_tracker: ox_agent::clarification_tracker::SharedClarificationTracker,
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

// ---------------------------------------------------------------------------
// FederationState
//
// The federation admin + query paths only need three pieces of
// shared state: the persistent store, the per-workspace adapter
// resolver cache, and the secret resolver. Carving those out into
// their own state lets the handlers declare their dependencies
// precisely (a federation handler cannot accidentally reach for
// the chat `Brain` or the model router) and lets tests build a
// real handler input without populating 25+ unrelated fields on
// `AppState`.
//
// Axum's `FromRef<AppState>` impl makes the extraction transparent:
// a handler typed as `State(FederationState)` extracts directly
// against the live `AppState` the server wires in at startup.
// ---------------------------------------------------------------------------

/// Narrow view of `AppState` carrying only what the federation
/// admin handlers and the federation-backed query handler touch.
///
/// This state is constructed once at startup (indirectly, by
/// `AppState`) and cheaply cloned per request — every field is
/// already an `Arc`, so clones share backing storage.
#[derive(Clone)]
pub struct FederationState {
    pub store: Arc<dyn Store>,
    pub federation_resolvers: Arc<DashMap<Uuid, Arc<WorkspaceResolverSlot>>>,
    pub secret_resolver: Arc<dyn SecretResolver>,
}

impl FromRef<AppState> for FederationState {
    fn from_ref(app: &AppState) -> Self {
        Self {
            store: Arc::clone(&app.store),
            federation_resolvers: Arc::clone(&app.federation_resolvers),
            secret_resolver: Arc::clone(&app.secret_resolver),
        }
    }
}

/// Per-workspace federation adapter slot.
///
/// Hydration is singleflight: the first request for a workspace
/// runs `list_data_sources + build_adapter × N` to populate the
/// inner resolver; concurrent first-requests await the same
/// initialisation future instead of each rebuilding the adapter
/// graph. Subsequent register / delete mutate the inner resolver
/// through its own `RwLock` — the `OnceCell` commits only once per
/// slot lifetime, and the lock handles post-hydration mutation.
///
/// A `refresh` operation throws the slot away (the outer `DashMap`
/// entry is removed) so the next access starts a fresh `OnceCell`.
pub struct WorkspaceResolverSlot {
    /// Populated exactly once per workspace slot. The lock inside
    /// the cell keeps post-hydration mutations possible without
    /// re-initialising the cell.
    inner: OnceCell<RwLock<InMemoryAdapterResolver>>,
}

impl WorkspaceResolverSlot {
    pub fn new() -> Self {
        Self {
            inner: OnceCell::new(),
        }
    }

    /// Access the hydrated resolver, initialising it exactly once
    /// through `init`. Concurrent callers see the same future.
    pub async fn get_or_init<F, Fut, E>(&self, init: F) -> Result<&RwLock<InMemoryAdapterResolver>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<InMemoryAdapterResolver, E>>,
    {
        self.inner
            .get_or_try_init(|| async {
                let resolver = init().await?;
                Ok::<_, E>(RwLock::new(resolver))
            })
            .await
    }

    /// Whether this slot has been hydrated. Useful for the admin
    /// `/health` endpoint to report a cold vs warm workspace
    /// without triggering lazy initialisation.
    pub fn is_hydrated(&self) -> bool {
        self.inner.initialized()
    }

    /// Returns a reference to the hydrated RwLock, or `None` when
    /// the slot is cold. Callers that need the hydrated path
    /// (writers) should prefer [`get_or_init`].
    pub fn get(&self) -> Option<&RwLock<InMemoryAdapterResolver>> {
        self.inner.get()
    }
}

impl Default for WorkspaceResolverSlot {
    fn default() -> Self {
        Self::new()
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

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic, clippy::unreachable))]
// Binary entrypoint: startup-time `expect` on infrastructure (OTLP,
// Prometheus, signal handlers) is idiomatic — failing fast is the correct
// behavior when the process cannot initialize. The library crate
// (`ox_api`) is still held to the stricter rule via workspace-level
// clippy config.
#![allow(clippy::expect_used)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use ox_brain::DefaultBrain;
use ox_brain::prompts::PromptRegistry;
use ox_runtime::registry::{GraphBackendConfig, GraphBackendRegistry};
use ox_source::registry::AdapterRegistry;

// All shared modules live in `lib.rs`; consume them via the library crate
// so each module compiles once (not twice — once as `ox_api::*` and again
// as `ontosyx`-bin-local `crate::*`).
use ox_api::config::OxConfig;
use ox_api::middleware::RateLimiter;
use ox_api::state::{AppState, Timeouts};
use ox_api::{
    collaboration, mcp, middleware, model_router, openapi, routes, schedule, sso, state,
    system_config,
};
use ox_api::spawn_scoped::spawn_system;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = OxConfig::load()?;
    config.validate()?;

    // Initialize tunables owned by non-api crates before any of their
    // code paths can lazily default them.
    ox_compiler::cypher::schema::init_auto_index_config(
        ox_compiler::cypher::schema::AutoIndexConfig {
            max_indices: config.cypher.max_auto_indices,
            high_priority_names: config.cypher.high_priority_names.clone(),
        },
    );

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    // Build optional OpenTelemetry tracer
    let otel_tracer = if config.otel.enabled {
        use opentelemetry::trace::TracerProvider;
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::Resource;

        let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.otel.endpoint)
            .build()
            .expect("Failed to create OTLP exporter");

        let resource = Resource::builder()
            .with_service_name(config.otel.service_name.clone())
            .build();

        let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(otlp_exporter)
            .with_resource(resource)
            .build();

        let tracer = tracer_provider.tracer("ontosyx");
        // Leak the provider so it lives for the process lifetime.
        // The batch exporter must outlive all spans; the tracer holds a weak ref.
        // This is the standard pattern for long-running OTEL-instrumented servers.
        std::mem::forget(tracer_provider);
        Some(tracer)
    } else {
        None
    };

    // Initialize tracing subscriber with optional OTel layer
    match (config.logging.format.as_str(), otel_tracer) {
        ("json", Some(tracer)) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .with(tracing_opentelemetry::layer().with_tracer(tracer))
                .init();
        }
        ("json", None) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .init();
        }
        (_, Some(tracer)) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer())
                .with(tracing_opentelemetry::layer().with_tracer(tracer))
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer())
                .init();
        }
    }

    if config.otel.enabled {
        tracing::info!(
            endpoint = %config.otel.endpoint,
            service = %config.otel.service_name,
            "OpenTelemetry tracing enabled"
        );
    }

    tracing::info!(
        provider = %config.llm.provider,
        model = %config.llm.model,
        "Ontosyx configuration loaded"
    );

    // Create shared LLM client pool + model resolver
    let client_pool = Arc::new(ox_brain::client_pool::ClientPool::new());
    // Pre-warm the primary client
    client_pool.get_or_create(&config.llm).await?;
    if let Some(ref fast_cfg) = config.fast_llm {
        client_pool.get_or_create(fast_cfg).await?;
        tracing::info!(
            provider = %fast_cfg.provider,
            model = %fast_cfg.model,
            "Fast LLM client pre-warmed in pool"
        );
    }
    let model_resolver: Arc<dyn ox_brain::model_resolver::ModelResolver> =
        Arc::new(ox_brain::model_resolver::StaticModelResolver::from_configs(
            &config.llm,
            config.fast_llm.as_ref(),
        ));

    // Create graph compiler + runtime via backend registry
    let graph_registry = GraphBackendRegistry::with_defaults();
    let graph_backend = graph_registry
        .create(
            &config.graph.backend,
            GraphBackendConfig {
                uri: config.graph.uri.clone(),
                username: config.graph.username.clone(),
                password: config.graph.password.clone(),
                database: config.graph.database.clone(),
                max_connections: config.graph.max_connections,
                load_concurrency: config.graph.load_concurrency,
                retry_max: config.graph.retry_max,
                retry_initial_delay_ms: config.graph.retry_initial_delay_ms,
                retry_max_delay_ms: config.graph.retry_max_delay_ms,
                isolation_strategy: config.graph.isolation_strategy.clone(),
                region: config.graph.region.clone(),
            },
        )
        .await?;
    // Wrap the registry compiler in a `PlanCache` so dashboard-style
    // repeated compiles of the same QueryIR hit memo instead of
    // re-emitting Cypher. The wrapper's `GraphCompiler` impl delegates
    // everything except `compile_query`; schema + load paths are
    // one-shot and don't benefit from caching.
    //
    // We clone the Arc into two typed handles: one as `Arc<dyn GraphCompiler>`
    // for handlers (unchanged API), one as `Arc<dyn PlanCacheHandle>` for
    // the `/metrics` endpoint and the ontology-save hook that needs to
    // invalidate the cache after a schema change.
    let raw_compiler = graph_backend.compiler;
    let plan_cache_arc = std::sync::Arc::new(ox_compiler::PlanCache::with_default_capacity(
        raw_compiler,
    ));
    let compiler: std::sync::Arc<dyn ox_compiler::GraphCompiler> = plan_cache_arc.clone();
    let plan_cache: Option<std::sync::Arc<dyn ox_compiler::PlanCacheHandle>> =
        Some(plan_cache_arc);
    let runtime = graph_backend.runtime;

    // Optional read-only runtime — used by MCP `execute_cypher` so a
    // bypass of the keyword heuristic still cannot mutate data. The
    // operator must create the DB user with SELECT-only privileges
    // (Neo4j: `CREATE USER readonly SET PASSWORD '…'; GRANT MATCH {*} ON GRAPH * TO readonly;`).
    let readonly_runtime = match (
        config.graph.readonly_user.as_deref(),
        config.graph.readonly_password.as_deref(),
    ) {
        (Some(user), Some(password)) if !user.is_empty() && !password.is_empty() => {
            let backend = graph_registry
                .create(
                    &config.graph.backend,
                    GraphBackendConfig {
                        uri: config.graph.uri.clone(),
                        username: user.to_string(),
                        password: password.to_string(),
                        database: config.graph.database.clone(),
                        max_connections: config.graph.max_connections,
                        load_concurrency: config.graph.load_concurrency,
                        retry_max: config.graph.retry_max,
                        retry_initial_delay_ms: config.graph.retry_initial_delay_ms,
                        retry_max_delay_ms: config.graph.retry_max_delay_ms,
                        isolation_strategy: config.graph.isolation_strategy.clone(),
                        region: config.graph.region.clone(),
                    },
                )
                .await?;
            tracing::info!(
                user,
                "Read-only graph runtime initialized — MCP execute_cypher will use it"
            );
            Some(backend.runtime)
        }
        _ => None,
    };
    let readonly_runtime = readonly_runtime.flatten();

    // Connect to PostgreSQL (required — fail if unavailable)
    let pg_store = ox_store::PostgresStore::connect_with_min(
        &config.postgres.url,
        config.postgres.max_connections,
        config.postgres.min_connections,
    )
    .await?;
    pg_store.migrate().await?;
    // Grab the pool reference before wrapping in Arc<dyn Store> for vector store sharing
    let shared_pg_pool = pg_store.pool().clone();
    let store = Arc::new(pg_store) as Arc<dyn ox_store::Store>;

    // Load prompt templates from DB (seeds from TOML on first run).
    // Uses SYSTEM_BYPASS to skip RLS during startup seeding.
    let toml_seed_dir = std::path::Path::new(&config.prompts.dir);
    let prompts = ox_store::PostgresStore::with_system_bypass(|| {
        PromptRegistry::load_from_db(store.as_ref(), Some(toml_seed_dir))
    })
    .await?;

    // Brain is created here but memory is attached later (after embedding init)
    let brain_base = DefaultBrain::new(
        Arc::clone(&client_pool),
        Arc::clone(&model_resolver),
        prompts,
        ox_brain::ProviderInfo {
            name: config.llm.provider.clone(),
            model: config.llm.model.clone(),
        },
    );

    // Initialize authentication
    let jwt_enabled = config.auth.jwt_secret.is_some();
    if jwt_enabled {
        tracing::info!(
            session_hours = config.auth.session_hours,
            "JWT authentication enabled"
        );
    } else {
        tracing::warn!(
            "JWT authentication disabled — only DB-backed API keys (X-API-Key) will work. \
             Set OX_AUTH__JWT_SECRET to enable SSO/JWT login."
        );
    }
    // First-boot bootstrap: if `api_keys` is empty AND OX_AUTH__BOOTSTRAP_KEY
    // is set, seed one row so a fresh deployment is reachable. Operators
    // should rotate the bootstrap key immediately.
    if let Some(plaintext) = config.auth.bootstrap_key.as_deref() {
        ox_store::SYSTEM_BYPASS
            .scope(true, async {
                match store.list_api_keys().await {
                    Ok(keys) if keys.is_empty() => {
                        let hash = ox_store::secret_token::secret_hash_sha256(plaintext.as_bytes());
                        if let Err(e) = store
                            .insert_api_key(
                                "bootstrap",
                                None,
                                "system:bootstrap",
                                &hash,
                                // Bootstrap needs full access to create the
                                // first workspace / additional keys. All
                                // other keys default to `viewer` at the DB
                                // level and must be escalated deliberately.
                                "admin",
                            )
                            .await
                        {
                            tracing::error!(error = %e, "Bootstrap api_key seed failed");
                        } else {
                            tracing::warn!(
                                "Seeded `bootstrap` api_key from OX_AUTH__BOOTSTRAP_KEY — \
                                 rotate this key immediately via the admin API"
                            );
                        }
                    }
                    Ok(_) => {
                        // Table is non-empty; ignore the bootstrap value.
                        tracing::info!("OX_AUTH__BOOTSTRAP_KEY ignored — api_keys already seeded");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Could not check api_keys for bootstrap seed");
                    }
                }
            })
            .await;
    }

    let timeouts = Timeouts::from(&config.timeouts);
    let repo_policy = state::RepoPolicy {
        allowed_roots: config.server.allowed_repo_roots.clone(),
        allowed_git_hosts: config.server.allowed_git_hosts.clone(),
    };
    if repo_policy.allowed_roots.is_empty() {
        tracing::info!("Repo enrichment: no allowed_repo_roots configured — local paths disabled");
    } else {
        tracing::info!(
            "Repo enrichment: allowed roots {:?}",
            repo_policy.allowed_roots
        );
    }
    if repo_policy.allowed_git_hosts.is_empty() {
        tracing::info!("Repo enrichment: no allowed_git_hosts configured — git URLs disabled");
    } else {
        tracing::info!(
            "Repo enrichment: allowed git hosts {:?}",
            repo_policy.allowed_git_hosts
        );
    }
    if config.server.allowed_secret_file_roots.is_empty() {
        tracing::warn!(
            "file: secret-ref sandbox is OPEN — every absolute path the server can \
             read is a valid secret_ref. Set server.allowed_secret_file_roots in \
             config.toml for multi-tenant deployments."
        );
    } else {
        tracing::info!(
            "file: secret-ref sandbox roots: {:?}",
            config.server.allowed_secret_file_roots
        );
    }
    let adapter_registry = Arc::new(AdapterRegistry::with_defaults());

    // Load runtime-tunable config from DB (falls back to defaults if unavailable)
    let system_config = Arc::new(tokio::sync::RwLock::new(
        system_config::load_system_config(store.as_ref()).await,
    ));
    let cancel_token = tokio_util::sync::CancellationToken::new();
    system_config::spawn_config_refresh(
        Arc::clone(&system_config),
        Arc::clone(&store),
        cancel_token.clone(),
    );
    // Daily stale-concept scan — proposes deprecations for ontology
    // types unused beyond the 6-month cutoff. Advisory only; the
    // admin dashboard flips the decision.
    ox_api::background::spawn_stale_concept_scan(
        Arc::clone(&store),
        cancel_token.clone(),
    );

    // Daily quality-baseline scan — writes the `median ± k·MAD`
    // snapshot per workspace so the banner can switch from its
    // hardcoded prior to adaptive thresholds (Phase B). Running
    // from day one ensures the table warms up before Phase B
    // activates.
    ox_api::background::spawn_quality_baseline_scan(
        Arc::clone(&store),
        cancel_token.clone(),
    );

    // Rate limiter (optional, controlled by config)
    let rate_limiter = if config.rate_limit.enabled {
        let rl = Arc::new(RateLimiter::new(&config.rate_limit));
        rl.spawn_cleanup_task(cancel_token.clone());
        tracing::info!(
            requests_per_window = config.rate_limit.requests_per_window,
            window_secs = config.rate_limit.window_secs,
            "Per-user rate limiting enabled"
        );
        Some(rl)
    } else {
        tracing::info!("Rate limiting disabled");
        None
    };

    // Build branchforge Auth from LLM config (used for Agent chat)
    // Uses shared resolve_auth() to stay consistent with Brain client creation.
    let agent_auth = client_pool.resolved_auth(&config.llm).await?;

    // Initialize semantic memory (embedding + pgvector)
    let memory = {
        let ec = &config.embedding;
        let embedder: Arc<dyn ox_memory::EmbeddingProvider> = match ec.provider.as_str() {
            "onnx" => {
                let model_dir = expand_tilde(&ec.model);
                if !model_dir.exists() {
                    anyhow::bail!(
                        "ONNX model directory not found: {} (from config: '{}')",
                        model_dir.display(),
                        ec.model,
                    );
                }
                tracing::info!(path = %model_dir.display(), "Loading ONNX embedding model…");
                let provider = ox_memory::OnnxEmbeddingProvider::load(&model_dir)?;
                Arc::new(provider)
            }
            _ => {
                if ec.provider != "noop" {
                    tracing::warn!(
                        provider = %ec.provider,
                        "Unknown embedding provider — falling back to noop"
                    );
                }
                Arc::new(ox_memory::NoopEmbeddingProvider::new(ec.dimensions))
            }
        };
        // Use provider-detected dimensions (ONNX auto-detects from model)
        let dims = embedder.dimensions();
        // Share the main PostgreSQL pool instead of creating a separate one
        let vector_store = ox_memory::PgVectorStore::new(shared_pg_pool.clone(), dims);
        let vectors: Arc<dyn ox_memory::VectorStore> = Arc::new(vector_store);
        tracing::info!(
            provider = embedder.provider_name(),
            model = %ec.model,
            dimensions = dims,
            "Semantic memory initialized"
        );
        Some(Arc::new(ox_memory::MemoryStore::new(embedder, vectors)))
    };

    // Attach memory store (schema RAG) and knowledge store (failure-driven corrections) to brain
    let kb_store = Arc::clone(&store) as Arc<dyn ox_store::KnowledgeStore>;
    let brain: Arc<dyn ox_brain::Brain> = if let Some(ref mem) = memory {
        Arc::new(
            brain_base
                .with_memory(Arc::clone(mem), None)
                .with_knowledge(kb_store),
        )
    } else {
        Arc::new(brain_base.with_knowledge(kb_store))
    };

    // Initialize OIDC providers (auto-discovers from issuer URLs)
    let oidc_providers = {
        let provider_configs = config.auth.providers.clone();
        if provider_configs.is_empty() {
            tracing::info!("No OIDC providers configured — SSO disabled");
            Arc::new(sso::OidcProviderRegistry::empty())
        } else {
            let registry = sso::OidcProviderRegistry::from_configs(provider_configs).await;
            let names = registry.provider_names();
            tracing::info!(providers = ?names, "OIDC providers initialized");
            Arc::new(registry)
        }
    };

    let db_model_router = Arc::new(model_router::DbModelRouter::new(Arc::clone(&store)));

    // Process-wide clarification tracker — fed by ResolveAmbiguityTool
    // and read by QueryGraphTool so the Phase 4.6
    // `clarification_success_rate` signal flips when a query
    // lands shortly after an ambiguity resolution in the same
    // agent session. Shared with the background evict loop so
    // stale session entries don't accumulate forever.
    let clarification_tracker: Arc<
        ox_agent::clarification_tracker::ClarificationTracker,
    > = Arc::new(ox_agent::clarification_tracker::ClarificationTracker::new());

    let state = AppState {
        brain,
        compiler,
        plan_cache,
        runtime,
        readonly_runtime,
        store,
        timeouts,
        auth_config: config.auth.clone(),
        repo_policy,
        adapter_registry,
        federation_resolvers: Arc::new(dashmap::DashMap::new()),
        secret_resolver: ox_api::credential::secret_resolver_with_file_roots(
            config.server.allowed_secret_file_roots.clone(),
        ),
        system_config,
        rate_limiter,
        memory,
        client_pool,
        model_router: db_model_router,
        agent_auth,
        oidc_providers,
        tool_review_channels: Some(Arc::new(dashmap::DashMap::new())),
        collaboration: Arc::new(collaboration::CollaborationHub::new(
            config.collaboration.broadcast_buffer,
        )),
        dashboards: config.dashboards.clone(),
        recovery: config.recovery.clone(),
        agent: config.agent.clone(),
        stream_limiter: Arc::new(ox_api::stream_limiter::StreamLimiter::new(
            config.agent.max_concurrent_streams_per_user,
        )),
        clarification_tracker: Arc::clone(&clarification_tracker),
    };

    // Spawn the clarification-tracker evict loop. Runs every 30
    // minutes under `spawn_system` so the cancellation token
    // drains it on graceful shutdown.
    ox_api::background::spawn_clarification_evict(
        Arc::clone(&clarification_tracker),
        cancel_token.clone(),
    );

    // CORS policy: explicit origins required. No permissive fallback.
    //
    // Development: set OX_SERVER__CORS_ORIGINS to your frontend URL.
    // Production: always set explicit origins.
    let cors = if config.server.cors_origins.is_empty() {
        tracing::warn!(
            "CORS: no origins configured — only same-origin requests will be accepted. \
             Set OX_SERVER__CORS_ORIGINS for cross-origin access."
        );
        // Default: no CORS headers at all (browser enforces same-origin)
        CorsLayer::new()
    } else {
        let origins: Vec<_> = config
            .server
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        tracing::info!("CORS: allowing origins {:?}", config.server.cors_origins);
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::PATCH,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
                axum::http::header::COOKIE,
                axum::http::HeaderName::from_static("x-api-key"),
                axum::http::HeaderName::from_static("x-request-id"),
            ])
            .expose_headers([
                axum::http::HeaderName::from_static("x-request-id"),
                axum::http::HeaderName::from_static("x-ratelimit-limit"),
                axum::http::HeaderName::from_static("x-ratelimit-remaining"),
                axum::http::HeaderName::from_static("retry-after"),
            ])
    };

    // MCP (Model Context Protocol) server for AI agent tool access
    let mcp_router = if config.mcp.enabled {
        use rmcp::transport::streamable_http_server::{
            StreamableHttpService, session::local::LocalSessionManager,
        };

        let mcp_brain = Arc::clone(&state.brain);
        let mcp_compiler = Arc::clone(&state.compiler);
        let mcp_runtime = state.runtime.clone();
        let mcp_readonly_runtime = state.readonly_runtime.clone();
        let mcp_store = Arc::clone(&state.store);
        let mcp_call_timeout = state.timeouts.raw_query;
        let mcp_rate_limit = config.mcp.rate_limit.clone();
        let mcp_reject_high_cost = state.agent.reject_high_cost;

        let mcp_service = StreamableHttpService::new(
            move || {
                Ok(mcp::OntosyxMcpServer::new(
                    Arc::clone(&mcp_brain),
                    Arc::clone(&mcp_compiler),
                    mcp_runtime.clone(),
                    mcp_readonly_runtime.clone(),
                    Arc::clone(&mcp_store),
                    mcp_call_timeout,
                    &mcp_rate_limit,
                    mcp_reject_high_cost,
                ))
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        // MCP endpoint sits behind the same middleware stack as the
        // regular API surface: `require_auth` then `workspace_context`.
        //
        // Why `workspace_context` is load-bearing here: every MCP tool
        // body calls `self.store.*` directly (see `mcp.rs::do_*`). Without
        // the workspace task-local set, those calls would read *every*
        // workspace's ontologies — a cross-tenant bypass that predates
        // this commit. Adding the middleware scopes `WORKSPACE_ID` and
        // `GRAPH_WORKSPACE_ID` for the StreamableHttpService future, which
        // stays alive for the entire session — so every tool invocation
        // inside that session runs under RLS, and `find_ontology_by_name`
        // etc. only ever return rows owned by the caller's workspace.
        //
        // Machine principals (API keys) must send `X-Workspace-Id`;
        // interactive JWT callers fall back to their default workspace
        // per the middleware's existing logic.
        //
        // The inner rate limiter and `forbidden_cypher_keyword` heuristic
        // remain as defense-in-depth, but are no longer the only gate.
        tracing::info!("MCP server enabled at /mcp (auth + workspace_context required)");
        Some(
            Router::new()
                .nest_service("/mcp", mcp_service)
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::workspace_context,
                ))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::require_auth,
                )),
        )
    } else {
        tracing::info!("MCP server disabled");
        None
    };

    // Prometheus metrics recorder
    let prometheus_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus metrics recorder");
    tracing::info!("Prometheus metrics recorder installed");

    // OpenAPI spec + Swagger UI
    let swagger_ui = {
        use utoipa::OpenApi;
        use utoipa_swagger_ui::SwaggerUi;
        SwaggerUi::new("/api/docs").url("/api/openapi.json", openapi::ApiDoc::openapi())
    };

    // Snapshot the plan-cache handle separately so the metrics closure
    // can surface stats without holding the whole AppState. A None here
    // (no cache configured) simply skips the gauge push.
    let metrics_plan_cache = state.plan_cache.clone();
    let mut app = Router::new()
        .nest("/api", routes::router(state.clone()))
        .route(
            "/metrics",
            axum::routing::get(move || {
                let plan_cache = metrics_plan_cache.clone();
                let handle = prometheus_handle.clone();
                async move {
                    if let Some(cache) = plan_cache {
                        ox_api::metrics::record_plan_cache_stats(cache.stats());
                    }
                    handle.render()
                }
            }),
        )
        .merge(swagger_ui);

    if let Some(mcp_router) = mcp_router {
        app = app.merge(mcp_router);
    }

    // ---------------------------------------------------------------------------
    // Background maintenance tasks (must clone before state is moved into router)
    // ---------------------------------------------------------------------------

    // Hourly: memory cleanup + session cleanup + WIP project archival (retention from config)
    // All maintenance tasks use SYSTEM_BYPASS to access data across all workspaces.
    {
        let maintenance_store = Arc::clone(&state.store);
        let maintenance_memory = state.memory.clone();
        let maintenance_channels = state.tool_review_channels.clone();
        let maintenance_client_pool = Arc::clone(&state.client_pool);
        let memory_days = config.retention.memory_days;
        let session_days = config.retention.session_days;
        let wip_archive_days = config.retention.wip_archive_days;
        let wip_delete_days = config.retention.wip_delete_days;
        let token = cancel_token.clone();
        spawn_system(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Shutting down maintenance task");
                        break;
                    }
                    _ = interval.tick() => {
                        // Per-workspace breakdown across the whole cycle.
                        // Each maintenance method returns Vec<(workspace_id,
                        // count)>; we fold them into a per-(workspace, task)
                        // table and emit one audit row per affected workspace
                        // so workspace admins can answer "which system
                        // actions touched my data".
                        use std::collections::BTreeMap;
                        type PerWsCounts = BTreeMap<&'static str, u64>;
                        let mut per_ws: BTreeMap<uuid::Uuid, PerWsCounts> = BTreeMap::new();
                        // Memory entries are not workspace-keyed; track them
                        // alongside the per-workspace map under a `None` key.
                        let mut system_only: PerWsCounts = BTreeMap::new();

                        // Inline helper instead of a closure: a closure
                        // would mutably borrow `per_ws` for the entire async
                        // block, blocking the per-workspace iteration below.
                        async fn run_step(
                            store: &Arc<dyn ox_store::Store>,
                            per_ws: &mut std::collections::BTreeMap<uuid::Uuid, std::collections::BTreeMap<&'static str, u64>>,
                            task: &'static str,
                            future: impl std::future::Future<Output = ox_core::error::OxResult<Vec<(uuid::Uuid, u64)>>>,
                        ) {
                            let _ = store; // silence unused warning if all paths skip
                            match future.await {
                                Ok(rows) if !rows.is_empty() => {
                                    let total: u64 = rows.iter().map(|(_, n)| n).sum();
                                    for (ws, n) in rows {
                                        *per_ws.entry(ws).or_default().entry(task).or_insert(0) += n;
                                    }
                                    tracing::info!(task, count = total, "maintenance step");
                                }
                                Err(e) => tracing::warn!(task, error = %e, "maintenance step failed"),
                                _ => {}
                            }
                        }

                        ox_store::SYSTEM_BYPASS.scope(true, async {
                            if let Some(ref mem) = maintenance_memory {
                                match mem.cleanup_stale(memory_days).await {
                                    Ok(n) if n > 0 => {
                                        *system_only.entry("memory_entries").or_insert(0) += n;
                                        tracing::info!(count = n, days = memory_days, "Cleaned stale memory entries");
                                    }
                                    Err(e) => tracing::warn!(error = %e, "Memory cleanup failed"),
                                    _ => {}
                                }
                            }
                            run_step(&maintenance_store, &mut per_ws, "agent_sessions",
                                maintenance_store.cleanup_old_sessions(session_days)).await;
                            run_step(&maintenance_store, &mut per_ws, "archived_projects",
                                maintenance_store.archive_stale_projects(wip_archive_days)).await;
                            run_step(&maintenance_store, &mut per_ws, "deleted_projects",
                                maintenance_store.delete_archived_projects(wip_delete_days)).await;
                            run_step(&maintenance_store, &mut per_ws, "analysis_results",
                                maintenance_store.cleanup_old_results(session_days)).await;
                            run_step(&maintenance_store, &mut per_ws, "expired_approvals",
                                maintenance_store.expire_old_approvals()).await;

                            // One audit row per affected workspace, plus one
                            // system-only row for memory cleanup if present.
                            for (ws_id, counts) in per_ws.iter() {
                                let summary: serde_json::Value = counts
                                    .iter()
                                    .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                                    .collect();
                                if let Err(e) = maintenance_store
                                    .record_audit_for_workspace(
                                        None,
                                        Some(*ws_id),
                                        "system_maintenance",
                                        "scheduled_task",
                                        None,
                                        summary,
                                    )
                                    .await
                                {
                                    tracing::warn!(error = %e, workspace_id = %ws_id, "Failed to record per-workspace maintenance audit");
                                }
                            }
                            if !system_only.is_empty() {
                                let summary: serde_json::Value = system_only
                                    .iter()
                                    .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                                    .collect();
                                if let Err(e) = maintenance_store
                                    .record_audit_for_workspace(
                                        None,
                                        None,
                                        "system_maintenance",
                                        "scheduled_task",
                                        None,
                                        summary,
                                    )
                                    .await
                                {
                                    tracing::warn!(error = %e, "Failed to record system-only maintenance audit");
                                }
                            }
                        }).await;

                        // Evict LLM clients idle for over 1 hour (Phase 4.9).
                        maintenance_client_pool.invalidate_idle(3600);

                        // Clean up stale tool review channels (abandoned sessions).
                        // Oneshot senders are removed on consumption; this handles
                        // entries that were never consumed (e.g., disconnected clients).
                        if let Some(ref channels) = maintenance_channels {
                            let count = channels.len();
                            if count > 1000 {
                                channels.clear();
                                tracing::info!(cleared = count, "Cleared stale tool review channels");
                            }
                        }
                    }
                }
            }
        });
    }

    // Periodic: retry failed embeddings (interval from config)
    if let Some(ref memory_for_retry) = state.memory {
        let retry_store = Arc::clone(&state.store);
        let retry_memory = Arc::clone(memory_for_retry);
        let retry_interval = config.retention.retry_interval_secs;
        let token = cancel_token.clone();
        spawn_system(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(retry_interval));
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Shutting down embedding retry task");
                        break;
                    }
                    _ = interval.tick() => {
                        ox_store::SYSTEM_BYPASS.scope(true, async {
                            match retry_store.list_pending_embeddings(10).await {
                                Ok(pending) => {
                                    for p in pending {
                                        let metadata: ox_memory::MemoryMetadata =
                                            match serde_json::from_value(p.metadata.clone()) {
                                                Ok(m) => m,
                                                Err(_) => {
                                                    let _ = retry_store
                                                        .delete_pending_embedding(p.id)
                                                        .await;
                                                    continue;
                                                }
                                            };
                                        let entry = ox_memory::MemoryEntry {
                                            id: format!("mem_retry_{}", p.id),
                                            content: p.content.clone(),
                                            metadata,
                                        };
                                        match retry_memory.store(entry).await {
                                            Ok(()) => {
                                                let _ = retry_store
                                                    .delete_pending_embedding(p.id)
                                                    .await;
                                                tracing::info!(id = %p.id, "Retry embedding succeeded");
                                            }
                                            Err(e) => {
                                                let _ = retry_store
                                                    .mark_embedding_failed(p.id, &e.to_string())
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to list pending embeddings")
                                }
                            }
                        }).await;
                    }
                }
            }
        });
    }

    // Scheduled recipe execution (check every 60 seconds)
    {
        let task_store = Arc::clone(&state.store);
        let analysis_timeout = state.timeouts.analysis;
        let token = cancel_token.clone();
        // Phase 4.11: prevent the same task from spawning twice when a
        // long-running execution overlaps the next 60-second poll.
        let in_flight: Arc<dashmap::DashSet<uuid::Uuid>> = Arc::new(dashmap::DashSet::new());
        spawn_system(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Shutting down scheduled recipe execution task");
                        break;
                    }
                    _ = interval.tick() => {
                        // System bypass: scheduled tasks need cross-workspace access
                        // to list due tasks and persist results.
                        let tasks = ox_store::SYSTEM_BYPASS.scope(true,
                            task_store.list_due_tasks()
                        ).await;
                        match tasks {
                            Ok(tasks) => {
                                for task in tasks {
                                    // Skip tasks already executing from a prior poll
                                    if !in_flight.insert(task.id) {
                                        tracing::debug!(task_id = %task.id, "Skipping in-flight scheduled task");
                                        continue;
                                    }
                                    let flight = Arc::clone(&in_flight);
                                    let store = Arc::clone(&task_store);
                                    // Individual task runs are NOT cancelled on shutdown.
                                    // Each run is bounded by analysis_timeout, and completing
                                    // in-flight work avoids result loss.
                                    // `spawn_system` wraps the future in SYSTEM_BYPASS so
                                    // RLS treats it as cross-workspace work.
                                    spawn_system(async move {
                                        tracing::info!(
                                            task_id = %task.id,
                                            recipe_id = %task.recipe_id,
                                            "Executing scheduled task"
                                        );

                                        // Load recipe
                                        let recipe = match store.get_recipe(task.recipe_id).await {
                                            Ok(Some(r)) => r,
                                            _ => {
                                                let fallback = chrono::Utc::now()
                                                    + chrono::Duration::hours(1);
                                                let _ = store
                                                    .update_task_after_run(task.id, fallback, "error")
                                                    .await;
                                                return;
                                            }
                                        };

                                        let next = schedule::next_run_from_cron(
                                            &task.cron_expression,
                                            chrono::Utc::now(),
                                        )
                                        .unwrap_or(
                                            chrono::Utc::now() + chrono::Duration::hours(1),
                                        );

                                        match ox_agent::tools::run_analysis_sandbox(
                                            &recipe.code_template,
                                            None,
                                            analysis_timeout,
                                        )
                                        .await
                                        {
                                            Ok(result) => {
                                                // Persist the result for auditing
                                                let analysis_result = ox_store::AnalysisResult {
                                                    id: uuid::Uuid::new_v4(),
                                                    recipe_id: Some(task.recipe_id),
                                                    ontology_lineage_id: None,
                                                    input_hash: String::new(),
                                                    output: serde_json::json!({
                                                        "stdout": result.stdout,
                                                        "stderr": result.stderr,
                                                        "exit_code": result.exit_code,
                                                        "scheduled_task_id": task.id.to_string(),
                                                    }),
                                                    duration_ms: 0,
                                                    created_at: chrono::Utc::now(),
                                                };
                                                if let Err(e) = store.create_analysis_result(&analysis_result).await {
                                                    tracing::warn!(error = %e, "Failed to save scheduled analysis result");
                                                }

                                                let status = if result.exit_code == 0 {
                                                    "completed"
                                                } else {
                                                    tracing::warn!(
                                                        task_id = %task.id,
                                                        exit_code = result.exit_code,
                                                        stderr = %result.stderr,
                                                        "Scheduled analysis exited with non-zero code"
                                                    );
                                                    "error"
                                                };
                                                let _ = store
                                                    .update_task_after_run(task.id, next, status)
                                                    .await;
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    task_id = %task.id,
                                                    error = %e,
                                                    "Scheduled analysis sandbox failed"
                                                );
                                                let _ = store
                                                    .update_task_after_run(task.id, next, "error")
                                                    .await;
                                            }
                                        }

                                        tracing::info!(
                                            task_id = %task.id,
                                            next_run = %next,
                                            "Scheduled task run finished"
                                        );
                                        // Release in-flight guard so next poll can re-schedule.
                                        flight.remove(&task.id);
                                    });
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, "Failed to list due tasks"),
                        }
                    }
                }
            }
        });
    }

    // Quality rule evaluation (check every 5 minutes)
    {
        let quality_store = Arc::clone(&state.store);
        let quality_runtime = state.runtime.clone();
        let token = cancel_token.clone();
        spawn_system(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Shutting down quality evaluation task");
                        break;
                    }
                    _ = interval.tick() => {
                        // The sweep may run Cypher via runtime.execute_query
                        // (custom rules) — that path expects BOTH the
                        // store task-local (for RLS on saved_ontologies /
                        // quality_* tables) AND the graph task-local (for
                        // run_pre_execute's workspace_scope gate). Scope
                        // both so every downstream hop sees a consistent
                        // system-bypass.
                        ox_store::SYSTEM_BYPASS.scope(true, ox_runtime::GRAPH_SYSTEM_BYPASS.scope(true, async {
                            evaluate_quality_rules(&quality_store, &quality_runtime).await;
                        })).await;
                    }
                }
            }
        });
    }

    // ---------------------------------------------------------------------------
    // Finalize router layers (state consumed here)
    // ---------------------------------------------------------------------------

    let app = app
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024)) // 2 MB
        .layer(axum::middleware::from_fn_with_state(
            state,
            middleware::rate_limit,
        ))
        .layer(axum::middleware::from_fn(middleware::inject_request_id))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // ---------------------------------------------------------------------------
    // Start server
    // ---------------------------------------------------------------------------

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .map_err(|e| {
            anyhow::anyhow!(
                "Invalid server address '{}:{}': {e}",
                config.server.host,
                config.server.port,
            )
        })?;
    tracing::info!("Listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token))
        .await?;

    tracing::info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal(cancel_token: tokio_util::sync::CancellationToken) {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received Ctrl+C, starting graceful shutdown"); }
        _ = terminate => { tracing::info!("Received SIGTERM, starting graceful shutdown"); }
    }

    cancel_token.cancel();
}

/// Expand `~/...` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

// ---------------------------------------------------------------------------
// Quality rule evaluation engine
// ---------------------------------------------------------------------------

async fn evaluate_quality_rules(
    store: &Arc<dyn ox_store::Store>,
    runtime: &Option<Arc<dyn ox_runtime::GraphRuntime>>,
) {
    let runtime = match runtime {
        Some(r) => r,
        None => return, // No graph runtime, skip
    };

    let rules = match store.list_quality_rules(None, None).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list quality rules");
            return;
        }
    };

    // Cache the resolved ontology per lineage so a sweep that evaluates
    // many rules against the same ontology only loads + deserialises
    // once. Cache hits share the same Arc; misses skip lineage-specific
    // validation but still run safety + workspace-scope.
    let mut ontology_cache: std::collections::HashMap<
        String,
        Option<Arc<ox_ontology::ir::OntologyIR>>,
    > = std::collections::HashMap::new();

    for rule in rules {
        if !rule.is_active {
            continue;
        }

        let ontology = match ontology_cache.get(&rule.ontology_lineage_id) {
            Some(cached) => cached.clone(),
            None => {
                // Lineage → identity → current version → hydrated IR.
                // Three separate failure modes (unknown lineage, no
                // committed version, hydrate error) all degrade to a
                // `None` here — rules still evaluate, they just skip
                // the label-conformance gate.
                let fetched = match store
                    .find_ontology_by_lineage(&rule.ontology_lineage_id)
                    .await
                {
                    Ok(Some(identity)) => match store.get_current_version(identity.id).await {
                        Ok(Some(version)) => match store.load_version(version.id).await {
                            Ok(ir) => {
                                tracing::info!(
                                    lineage = %rule.ontology_lineage_id,
                                    version = %version.version,
                                    "Quality sweep loaded ontology"
                                );
                                Some(Arc::new(ir))
                            }
                            Err(e) => {
                                tracing::warn!(
                                    lineage = %rule.ontology_lineage_id,
                                    version = %version.version,
                                    error = %e,
                                    "Quality sweep failed to hydrate ontology IR; rule runs without label validation"
                                );
                                None
                            }
                        },
                        Ok(None) => {
                            tracing::warn!(
                                lineage = %rule.ontology_lineage_id,
                                "Quality sweep: lineage has no committed version"
                            );
                            None
                        }
                        Err(e) => {
                            tracing::warn!(
                                lineage = %rule.ontology_lineage_id,
                                error = %e,
                                "Quality sweep current-version lookup failed"
                            );
                            None
                        }
                    },
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!(
                            lineage = %rule.ontology_lineage_id,
                            error = %e,
                            "Quality sweep lineage lookup failed"
                        );
                        None
                    }
                };
                ontology_cache.insert(rule.ontology_lineage_id.clone(), fetched.clone());
                fetched
            }
        };

        let eval_fut = async {
            match rule.rule_type.as_str() {
                "completeness" => evaluate_completeness(runtime, &rule).await,
                "uniqueness" => evaluate_uniqueness(runtime, &rule).await,
                "custom" => evaluate_custom(runtime, &rule).await,
                _ => (true, None), // Unsupported types pass silently
            }
        };
        let (passed, actual_value) = match ontology {
            Some(onto) => ox_runtime::GRAPH_ONTOLOGY.scope(onto, eval_fut).await,
            None => eval_fut.await,
        };

        if !matches!(
            rule.rule_type.as_str(),
            "completeness" | "uniqueness" | "custom"
        ) {
            continue;
        }

        let result = ox_store::QualityResult {
            id: uuid::Uuid::new_v4(),
            workspace_id: rule.workspace_id,
            rule_id: rule.id,
            passed,
            actual_value,
            details: serde_json::json!({}),
            evaluated_at: chrono::Utc::now(),
        };

        if let Err(e) = store.record_quality_result(&result).await {
            tracing::warn!(rule_id = %rule.id, error = %e, "Failed to record quality result");
        }
    }
}

async fn evaluate_completeness(
    runtime: &Arc<dyn ox_runtime::GraphRuntime>,
    rule: &ox_store::QualityRule,
) -> (bool, Option<f64>) {
    let cypher = if let Some(ref prop) = rule.target_property {
        format!(
            "MATCH (n:{}) WITH count(n) AS total, count(n.{}) AS filled \
             RETURN CASE WHEN total = 0 THEN 100.0 ELSE filled * 100.0 / total END AS pct",
            rule.target_label, prop
        )
    } else {
        return (true, Some(100.0)); // No property specified
    };

    match runtime
        .execute_query(&cypher, &std::collections::HashMap::new())
        .await
    {
        Ok(result) => {
            if let Some(row) = result.rows.first()
                && let Some(ox_core::types::PropertyValue::Float(pct)) = row.first()
            {
                return (*pct >= rule.threshold, Some(*pct));
            }
            (true, None)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Quality completeness check failed");
            (false, None)
        }
    }
}

async fn evaluate_uniqueness(
    runtime: &Arc<dyn ox_runtime::GraphRuntime>,
    rule: &ox_store::QualityRule,
) -> (bool, Option<f64>) {
    let cypher = if let Some(ref prop) = rule.target_property {
        format!(
            "MATCH (n:{}) WITH count(n) AS total, count(DISTINCT n.{}) AS distinct_vals \
             RETURN CASE WHEN total = 0 THEN 100.0 ELSE distinct_vals * 100.0 / total END AS pct",
            rule.target_label, prop
        )
    } else {
        return (true, Some(100.0));
    };

    match runtime
        .execute_query(&cypher, &std::collections::HashMap::new())
        .await
    {
        Ok(result) => {
            if let Some(row) = result.rows.first()
                && let Some(ox_core::types::PropertyValue::Float(pct)) = row.first()
            {
                return (*pct >= rule.threshold, Some(*pct));
            }
            (true, None)
        }
        Err(_) => (false, None),
    }
}

async fn evaluate_custom(
    runtime: &Arc<dyn ox_runtime::GraphRuntime>,
    rule: &ox_store::QualityRule,
) -> (bool, Option<f64>) {
    let cypher = match &rule.cypher_check {
        Some(c) => c.clone(),
        None => return (true, None),
    };

    match runtime
        .execute_query(&cypher, &std::collections::HashMap::new())
        .await
    {
        Ok(result) => {
            // Custom queries should return a single numeric value
            if let Some(row) = result.rows.first() {
                if let Some(ox_core::types::PropertyValue::Float(val)) = row.first() {
                    return (*val >= rule.threshold, Some(*val));
                }
                if let Some(ox_core::types::PropertyValue::Int(val)) = row.first() {
                    let fval = *val as f64;
                    return (fval >= rule.threshold, Some(fval));
                }
            }
            (true, None)
        }
        Err(_) => (false, None),
    }
}

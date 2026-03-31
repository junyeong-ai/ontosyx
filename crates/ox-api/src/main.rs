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
use ox_source::registry::IntrospectorRegistry;

pub(crate) mod acl_enforcement;
pub(crate) mod audit_middleware;
pub(crate) mod collaboration;
mod config;
mod error;
mod mcp;
pub(crate) mod metrics;
mod middleware;
pub(crate) mod model_router;
pub(crate) mod openapi;
mod principal;
mod routes;
pub(crate) mod schedule;
pub(crate) mod spawn_scoped;
pub(crate) mod sso;
mod state;
pub(crate) mod system_config;
mod validation;
pub(crate) mod workspace;
pub(crate) mod workspace_scope;

use config::OxConfig;
use middleware::RateLimiter;
use state::{AppState, Timeouts};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = OxConfig::load()?;

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
            },
        )
        .await?;
    let compiler = graph_backend.compiler;
    let runtime = graph_backend.runtime;

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
    let api_key_enabled = config.auth.api_key.is_some();
    if jwt_enabled {
        tracing::info!(
            session_hours = config.auth.session_hours,
            "JWT authentication enabled"
        );
    }
    if api_key_enabled {
        tracing::info!("API key authentication enabled");
    }
    if !jwt_enabled && !api_key_enabled {
        tracing::warn!(
            "No authentication configured — protected endpoints will reject all requests. \
             Set OX_AUTH__JWT_SECRET and/or OX_AUTH__API_KEY."
        );
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
    let introspector_registry = Arc::new(IntrospectorRegistry::with_defaults());

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

    // Attach memory store to brain for schema RAG in query translation
    let brain: Arc<dyn ox_brain::Brain> = if let Some(ref mem) = memory {
        Arc::new(brain_base.with_memory(Arc::clone(mem), None))
    } else {
        Arc::new(brain_base)
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

    let state = AppState {
        brain,
        compiler,
        runtime,
        store,
        timeouts,
        auth_config: config.auth.clone(),
        repo_policy,
        introspector_registry,
        system_config,
        rate_limiter,
        memory,
        client_pool,
        model_router: db_model_router,
        agent_auth,
        oidc_providers,
        tool_review_channels: Some(Arc::new(dashmap::DashMap::new())),
        collaboration: Arc::new(collaboration::CollaborationHub::new()),
    };

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
        let mcp_store = Arc::clone(&state.store);

        let mcp_service = StreamableHttpService::new(
            move || {
                Ok(mcp::OntosyxMcpServer::new(
                    Arc::clone(&mcp_brain),
                    Arc::clone(&mcp_compiler),
                    mcp_runtime.clone(),
                    Arc::clone(&mcp_store),
                ))
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        tracing::info!("MCP server enabled at /mcp");
        Some(Router::new().nest_service("/mcp", mcp_service))
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

    let mut app = Router::new()
        .nest("/api", routes::router(state.clone()))
        .route(
            "/metrics",
            axum::routing::get(move || async move { prometheus_handle.render() }),
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
        let memory_days = config.retention.memory_days;
        let session_days = config.retention.session_days;
        let wip_archive_days = config.retention.wip_archive_days;
        let wip_delete_days = config.retention.wip_delete_days;
        let token = cancel_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Shutting down maintenance task");
                        break;
                    }
                    _ = interval.tick() => {
                        ox_store::SYSTEM_BYPASS.scope(true, async {
                            if let Some(ref mem) = maintenance_memory {
                                match mem.cleanup_stale(memory_days).await {
                                    Ok(n) if n > 0 => {
                                        tracing::info!(count = n, days = memory_days, "Cleaned stale memory entries")
                                    }
                                    Err(e) => tracing::warn!(error = %e, "Memory cleanup failed"),
                                    _ => {}
                                }
                            }
                            match maintenance_store.cleanup_old_sessions(session_days).await {
                                Ok(n) if n > 0 => {
                                    tracing::info!(count = n, days = session_days, "Cleaned old agent sessions")
                                }
                                Err(e) => tracing::warn!(error = %e, "Session cleanup failed"),
                                _ => {}
                            }
                            match maintenance_store.archive_stale_projects(wip_archive_days).await {
                                Ok(n) if n > 0 => {
                                    tracing::info!(count = n, days = wip_archive_days, "Archived stale WIP projects")
                                }
                                Err(e) => tracing::warn!(error = %e, "WIP project archival failed"),
                                _ => {}
                            }
                            match maintenance_store.delete_archived_projects(wip_delete_days).await {
                                Ok(n) if n > 0 => {
                                    tracing::info!(count = n, days = wip_delete_days, "Deleted archived projects")
                                }
                                Err(e) => tracing::warn!(error = %e, "Archived project deletion failed"),
                                _ => {}
                            }
                            // Evict stale analysis results (same retention as sessions)
                            match maintenance_store.cleanup_old_results(session_days).await {
                                Ok(n) if n > 0 => {
                                    tracing::info!(count = n, days = session_days, "Cleaned old analysis results")
                                }
                                Err(e) => tracing::warn!(error = %e, "Analysis result cleanup failed"),
                                _ => {}
                            }
                            // Expire pending approvals past their deadline
                            match maintenance_store.expire_old_approvals().await {
                                Ok(n) if n > 0 => {
                                    tracing::info!(count = n, "Expired old pending approvals")
                                }
                                Err(e) => tracing::warn!(error = %e, "Approval expiry failed"),
                                _ => {}
                            }
                        }).await;

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
        tokio::spawn(async move {
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
        tokio::spawn(async move {
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
                                    let store = Arc::clone(&task_store);
                                    // Individual task runs are NOT cancelled on shutdown.
                                    // Each run is bounded by analysis_timeout, and completing
                                    // in-flight work avoids result loss.
                                    tokio::spawn(ox_store::SYSTEM_BYPASS.scope(true, async move {
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
                                                    ontology_id: None,
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
                                    }));
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
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Shutting down quality evaluation task");
                        break;
                    }
                    _ = interval.tick() => {
                        ox_store::SYSTEM_BYPASS.scope(true, async {
                            evaluate_quality_rules(&quality_store, &quality_runtime).await;
                        }).await;
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

    let rules = match store.list_quality_rules(None).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list quality rules");
            return;
        }
    };

    for rule in rules {
        if !rule.is_active {
            continue;
        }

        let (passed, actual_value) = match rule.rule_type.as_str() {
            "completeness" => evaluate_completeness(runtime, &rule).await,
            "uniqueness" => evaluate_uniqueness(runtime, &rule).await,
            "custom" => evaluate_custom(runtime, &rule).await,
            _ => continue, // Skip unsupported types
        };

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

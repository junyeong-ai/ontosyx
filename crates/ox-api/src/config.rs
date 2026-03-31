use std::fmt;

use config::{Config, Environment, File};
use serde::Deserialize;

/// Root configuration for the Ontosyx platform.
/// Loaded from `ontosyx.toml` (or path in `OX_CONFIG_FILE`) with
/// environment variable overrides using the `OX_` prefix.
///
/// Example env override: `OX_SERVER__PORT=8080` overrides `server.port`.
#[derive(Debug, Deserialize, Clone)]
pub struct OxConfig {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub graph: GraphConfig,
    pub postgres: PostgresConfig,
    pub llm: LlmProviderConfig,
    pub fast_llm: Option<LlmProviderConfig>,
    pub embedding: EmbeddingConfig,
    pub logging: LoggingConfig,
    pub prompts: PromptsConfig,
    pub timeouts: TimeoutsConfig,
    pub rate_limit: RateLimitConfig,
    pub retention: RetentionConfig,
    pub mcp: McpConfig,
    pub otel: OtelConfig,
}

/// Embedding model configuration for semantic memory.
#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingConfig {
    /// Provider: "onnx" or "noop" (default: "noop")
    #[serde(default = "default_embedding_provider")]
    pub provider: String,
    /// Model path (onnx: directory containing model.onnx + tokenizer.json)
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// Vector dimensions (default: 1024, auto-detected for onnx)
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,
}

fn default_embedding_provider() -> String {
    "noop".to_string()
}
fn default_embedding_model() -> String {
    String::new()
}
fn default_embedding_dimensions() -> usize {
    1024
}

#[derive(Deserialize, Clone)]
pub struct AuthConfig {
    /// JWT secret for signing/verifying platform tokens.
    /// Required in production; when unset, JWT auth is disabled (API key only).
    pub jwt_secret: Option<String>,
    /// Session duration in hours (default: 24).
    pub session_hours: u64,
    /// API key for programmatic/CI access.
    pub api_key: Option<String>,
    /// Email of the first user to be auto-promoted to admin.
    pub first_admin_email: Option<String>,
    /// OIDC providers. Each entry is auto-discovered from issuer_url.
    /// Supports Google, Microsoft, Okta, Auth0, Keycloak — any standard OIDC provider.
    #[serde(default)]
    pub providers: Vec<crate::sso::OidcProviderConfig>,
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthConfig")
            .field(
                "jwt_secret",
                &self.jwt_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("session_hours", &self.session_hours)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("first_admin_email", &self.first_admin_email)
            .field(
                "providers",
                &self.providers.iter().map(|p| &p.name).collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpConfig {
    /// Whether the MCP (Model Context Protocol) endpoint is enabled (default: true).
    /// When enabled, an MCP server is mounted at `/mcp` for AI agent tool access.
    pub enabled: bool,
}

/// OpenTelemetry tracing export configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct OtelConfig {
    /// Whether OpenTelemetry tracing export is enabled (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// OTLP endpoint URL (default: http://localhost:4317).
    #[serde(default = "default_otel_endpoint")]
    pub endpoint: String,
    /// Service name for traces (default: ontosyx).
    #[serde(default = "default_otel_service_name")]
    pub service_name: String,
}

fn default_otel_endpoint() -> String {
    "http://localhost:4317".to_string()
}
fn default_otel_service_name() -> String {
    "ontosyx".to_string()
}

/// Data retention policy for background cleanup tasks.
#[derive(Debug, Deserialize, Clone)]
pub struct RetentionConfig {
    /// Memory entries not accessed within this many days are deleted (default: 180).
    #[serde(default = "default_memory_days")]
    pub memory_days: i64,
    /// Agent sessions older than this many days are deleted (default: 90).
    #[serde(default = "default_session_days")]
    pub session_days: i64,
    /// Embedding retry interval in seconds (default: 300).
    #[serde(default = "default_retry_interval_secs")]
    pub retry_interval_secs: u64,
    /// WIP projects not updated within this many days are archived (default: 30).
    #[serde(default = "default_wip_archive_days")]
    pub wip_archive_days: i64,
    /// Archived projects older than this many days are permanently deleted (default: 90).
    #[serde(default = "default_wip_delete_days")]
    pub wip_delete_days: i64,
}

fn default_memory_days() -> i64 {
    180
}
fn default_session_days() -> i64 {
    90
}
fn default_retry_interval_secs() -> u64 {
    300
}
fn default_wip_archive_days() -> i64 {
    30
}
fn default_wip_delete_days() -> i64 {
    90
}

#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitConfig {
    /// Whether rate limiting is enabled (default: true)
    pub enabled: bool,
    /// Maximum requests per window per principal (default: 120)
    pub requests_per_window: u32,
    /// Window duration in seconds (default: 60)
    pub window_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TimeoutsConfig {
    /// Design/load LLM operation timeout in seconds (default: 300)
    pub design_operation_secs: u64,
    /// Raw query execution timeout in seconds (default: 30)
    pub raw_query_secs: u64,
    /// Health check timeout in seconds (default: 3)
    pub health_check_secs: u64,
    /// Analysis sandbox execution timeout in seconds (default: 120)
    pub analysis_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
    /// Allowed root directories for repo enrichment (local filesystem).
    /// If empty, local repo enrichment is disabled for safety.
    #[serde(default)]
    pub allowed_repo_roots: Vec<String>,
    /// Allowed Git hostnames for remote repo enrichment.
    /// If empty, git URL repo enrichment is disabled for safety.
    #[serde(default)]
    pub allowed_git_hosts: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct GraphConfig {
    /// Graph database backend: "neo4j" (future: "neptune", "memgraph")
    pub backend: String,
    pub uri: String,
    pub username: String,
    pub password: String,
    pub database: String,
    pub max_connections: u32,
    /// Max concurrent batches during load operations (default: 8)
    pub load_concurrency: Option<usize>,
    /// Maximum number of retries for transient graph errors (default: 3)
    pub retry_max: Option<u32>,
    /// Initial retry delay in milliseconds (default: 100)
    pub retry_initial_delay_ms: Option<u64>,
    /// Maximum retry delay in milliseconds (default: 5000)
    pub retry_max_delay_ms: Option<u64>,
    /// Workspace isolation strategy for graph data.
    /// "property" (default): adds _workspace_id property to nodes (Community-compatible)
    /// "database": uses separate Neo4j databases per workspace (Enterprise/DozerDB only)
    /// "none": no graph isolation (all workspaces share graph data)
    #[serde(default = "default_isolation_strategy")]
    pub isolation_strategy: String,
}

fn default_isolation_strategy() -> String {
    "property".to_string()
}

impl fmt::Debug for GraphConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphConfig")
            .field("backend", &self.backend)
            .field("uri", &self.uri)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("database", &self.database)
            .field("max_connections", &self.max_connections)
            .field("load_concurrency", &self.load_concurrency)
            .field("retry_max", &self.retry_max)
            .field("retry_initial_delay_ms", &self.retry_initial_delay_ms)
            .field("retry_max_delay_ms", &self.retry_max_delay_ms)
            .field("isolation_strategy", &self.isolation_strategy)
            .finish()
    }
}

#[derive(Deserialize, Clone)]
pub struct PostgresConfig {
    pub url: String,
    pub max_connections: u32,
    #[serde(default)]
    pub min_connections: u32,
}

impl fmt::Debug for PostgresConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresConfig")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

/// Re-export the canonical LLM provider config from ox-brain.
pub use ox_brain::auth::LlmProviderConfig;

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PromptsConfig {
    /// TOML seed directory for initial DB population.
    /// Only used when `prompt_templates` table is empty (first deployment).
    pub dir: String,
}

impl OxConfig {
    /// Load configuration with layered precedence:
    /// 1. Defaults (coded below)
    /// 2. TOML file (`ontosyx.toml` or `OX_CONFIG_FILE` env var)
    /// 3. Environment variables with `OX_` prefix (double underscore = nesting)
    pub fn load() -> anyhow::Result<Self> {
        let config_file =
            std::env::var("OX_CONFIG_FILE").unwrap_or_else(|_| "ontosyx.toml".to_string());

        let config = Config::builder()
            // Defaults
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3001_i64)?
            .set_default("server.cors_origins", Vec::<String>::new())?
            .set_default(
                "server.allowed_git_hosts",
                vec!["github.com", "gitlab.com", "bitbucket.org"],
            )?
            .set_default("auth.session_hours", 24_i64)?
            .set_default("graph.backend", "neo4j")?
            .set_default("graph.uri", "bolt://localhost:7687")?
            .set_default("graph.username", "neo4j")?
            .set_default("graph.password", "neo4j")?
            .set_default("graph.database", "neo4j")?
            .set_default("graph.max_connections", 16_i64)?
            .set_default("graph.isolation_strategy", "property")?
            .set_default(
                "postgres.url",
                "postgres://ontosyx:ontosyx-dev@localhost:5433/ontosyx",
            )?
            .set_default("postgres.max_connections", 10_i64)?
            .set_default("llm.provider", "anthropic")?
            .set_default("llm.model", "claude-sonnet-4-6")?
            .set_default("logging.level", "info")?
            .set_default("logging.format", "pretty")?
            .set_default("prompts.dir", "prompts")?
            .set_default("rate_limit.enabled", true)?
            .set_default("rate_limit.requests_per_window", 120_i64)?
            .set_default("rate_limit.window_secs", 60_i64)?
            .set_default("timeouts.design_operation_secs", 120_i64)?
            .set_default("timeouts.raw_query_secs", 30_i64)?
            .set_default("timeouts.health_check_secs", 3_i64)?
            .set_default("timeouts.analysis_secs", 120_i64)?
            .set_default("retention.memory_days", 180_i64)?
            .set_default("retention.session_days", 90_i64)?
            .set_default("retention.retry_interval_secs", 300_i64)?
            .set_default("retention.wip_archive_days", 30_i64)?
            .set_default("retention.wip_delete_days", 90_i64)?
            .set_default("mcp.enabled", true)?
            .set_default("otel.enabled", false)?
            .set_default("otel.endpoint", "http://localhost:4317")?
            .set_default("otel.service_name", "ontosyx")?
            // TOML file (optional — missing file is not an error)
            .add_source(File::with_name(&config_file).required(false))
            // Environment overrides: OX_SERVER__PORT=8080
            .add_source(
                Environment::with_prefix("OX")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        let ox: OxConfig = config.try_deserialize()?;
        Ok(ox)
    }
}

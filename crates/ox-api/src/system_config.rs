use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};

use ox_store::Store;

// ---------------------------------------------------------------------------
// SystemConfig — cached runtime-tunable configuration from the DB
//
// Layered precedence: code defaults → DB seed → runtime DB updates.
// All typed getters fall back to hardcoded defaults if the DB row is missing
// or unparseable, ensuring the system always has a valid configuration.
// ---------------------------------------------------------------------------

/// Refresh interval for background config reload.
const REFRESH_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// In-memory cache of `system_config` table rows.
///
/// Uses `"category.key"` composite string keys. Each `get()` call
/// allocates a single `format!` string for lookup — negligible with ~20 entries.
#[derive(Debug, Clone)]
pub struct SystemConfig {
    entries: HashMap<String, String>,
}

#[allow(dead_code)] // getters pre-defined for all DB seed keys; consumers added incrementally
impl SystemConfig {
    fn get(&self, category: &str, key: &str) -> Option<&str> {
        let composite = format!("{category}.{key}");
        self.entries.get(&composite).map(|s| s.as_str())
    }

    fn get_int(&self, category: &str, key: &str, default: i64) -> i64 {
        self.get(category, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn get_usize(&self, category: &str, key: &str, default: usize) -> usize {
        self.get(category, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn get_f64(&self, category: &str, key: &str, default: f64) -> f64 {
        self.get(category, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn get_string(&self, category: &str, key: &str, default: &str) -> String {
        self.get(category, key).unwrap_or(default).to_string()
    }

    // -- Schema complexity (structured output thresholds) --------------------

    pub fn schema_max_optional_params(&self) -> usize {
        self.get_usize("llm", "schema_max_optional", 24)
    }

    pub fn schema_max_total_properties(&self) -> usize {
        self.get_usize("llm", "schema_max_total", 50)
    }

    // -- LLM parameters -------------------------------------------------------

    pub fn design_ontology_max_tokens(&self) -> i64 {
        self.get_int("llm", "design_ontology_max_tokens", 16384)
    }

    pub fn design_ontology_temperature(&self) -> f64 {
        self.get_f64("llm", "design_ontology_temperature", 0.0)
    }

    pub fn refine_ontology_max_tokens(&self) -> i64 {
        self.get_int("llm", "refine_ontology_max_tokens", 16384)
    }

    pub fn refine_ontology_temperature(&self) -> f64 {
        self.get_f64("llm", "refine_ontology_temperature", 0.0)
    }

    // -- Thresholds -----------------------------------------------------------

    pub fn large_schema_warning_threshold(&self) -> usize {
        self.get_usize("thresholds", "large_schema_warning", 50)
    }

    pub fn large_schema_gate_threshold(&self) -> usize {
        self.get_usize("thresholds", "large_schema_gate", 100)
    }

    pub fn large_ontology_threshold(&self) -> usize {
        self.get_usize("thresholds", "large_ontology", 100)
    }

    pub fn max_design_tables(&self) -> usize {
        self.get_usize("thresholds", "max_design_tables", 40)
    }

    // -- Batch design (divide-and-conquer for all structured sources) --------

    /// Maximum number of tables per batch cluster in divide-and-conquer design.
    pub fn batch_size(&self) -> usize {
        self.get_usize("design", "batch_size", 15)
    }

    /// Design operation timeout (runtime-tunable).
    pub fn design_timeout_secs(&self) -> u64 {
        self.get_int("timeouts", "design_operation_secs", 120) as u64
    }

    /// HITL tool review approval timeout.
    pub fn tool_review_timeout_secs(&self) -> u64 {
        self.get_int("timeouts", "tool_review_secs", 120) as u64
    }

    // -- Timeouts (runtime-tunable overrides for static TOML values) ----------

    pub fn profiling_timeout_secs(&self) -> u64 {
        self.get_int("timeouts", "profiling_secs", 60) as u64
    }

    pub fn refine_timeout_secs(&self) -> u64 {
        self.get_int("timeouts", "refine_operation_secs", 300) as u64
    }

    // -- Profiling ------------------------------------------------------------

    pub fn max_distinct_values(&self) -> usize {
        self.get_usize("profiling", "max_distinct_values", 30)
    }

    pub fn large_schema_sample_values(&self) -> usize {
        self.get_usize("profiling", "large_schema_sample_values", 5)
    }

    pub fn large_schema_value_chars(&self) -> usize {
        self.get_usize("profiling", "large_schema_value_chars", 50)
    }

    pub fn large_ontology_sample_size(&self) -> usize {
        self.get_usize("profiling", "large_ontology_sample_size", 10)
    }

    pub fn large_ontology_concurrency(&self) -> usize {
        self.get_usize("profiling", "large_ontology_concurrency", 4)
    }

    // -- UI/frontend ----------------------------------------------------------

    pub fn elk_direction(&self) -> String {
        self.get_string("ui", "elk_direction", "RIGHT")
    }

    pub fn elk_node_spacing(&self) -> i64 {
        self.get_int("ui", "elk_node_spacing", 60)
    }

    pub fn elk_layer_spacing(&self) -> i64 {
        self.get_int("ui", "elk_layer_spacing", 100)
    }

    pub fn elk_edge_routing(&self) -> String {
        self.get_string("ui", "elk_edge_routing", "ORTHOGONAL")
    }

    pub fn worker_timeout_ms(&self) -> i64 {
        self.get_int("ui", "worker_timeout_ms", 10000)
    }
}

/// Load all system_config rows from the database into an in-memory cache.
/// Falls back to an empty config (all getters return defaults) if DB read fails.
pub async fn load_system_config(store: &dyn Store) -> SystemConfig {
    match store.get_all_config().await {
        Ok(rows) => {
            let count = rows.len();
            let entries: HashMap<String, String> = rows
                .into_iter()
                .map(|r| (format!("{}.{}", r.category, r.key), r.value))
                .collect();
            info!(config_entries = count, "System config loaded from database");
            SystemConfig { entries }
        }
        Err(e) => {
            warn!("Failed to load system config from database: {e} — using defaults");
            SystemConfig {
                entries: HashMap::new(),
            }
        }
    }
}

/// Spawn a background task that refreshes the system config cache periodically.
pub fn spawn_config_refresh(
    config: Arc<RwLock<SystemConfig>>,
    store: Arc<dyn Store>,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        // Skip the first tick (config was just loaded)
        interval.tick().await;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    info!("Shutting down config refresh task");
                    break;
                }
                _ = interval.tick() => {
                    let new_config = load_system_config(store.as_ref()).await;
                    *config.write().await = new_config;
                }
            }
        }
    });
}

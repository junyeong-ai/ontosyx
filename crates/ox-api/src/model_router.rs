use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use ox_brain::{
    auth::LlmProviderConfig,
    model_resolver::{ModelResolver, ResolvedModel},
};
use ox_core::error::{OxError, OxResult};
use ox_store::{Store, WORKSPACE_ID};

// ---------------------------------------------------------------------------
// DbModelRouter — DB-backed ModelResolver with TTL cache
// ---------------------------------------------------------------------------

/// Database-backed model resolver.
///
/// Resolution priority (handled by a single SQL query):
/// 1. Workspace + specific operation (highest)
/// 2. Workspace + wildcard (*)
/// 3. Global + specific operation
/// 4. Global + wildcard (*) (lowest)
///
/// Within each level, higher `priority` wins.
pub struct DbModelRouter {
    store: Arc<dyn Store>,
    fallback: Option<Arc<dyn ModelResolver>>,
    cache: Arc<RwLock<RouterCache>>,
    ttl: Duration,
}

struct RouterCache {
    entries: HashMap<(Option<Uuid>, String), (ResolvedModel, Instant)>,
}

impl DbModelRouter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self::with_fallback(store, Option::<Arc<dyn ModelResolver>>::None)
    }

    pub fn with_fallback(
        store: Arc<dyn Store>,
        fallback: impl Into<Option<Arc<dyn ModelResolver>>>,
    ) -> Self {
        Self {
            store,
            fallback: fallback.into(),
            cache: Arc::new(RwLock::new(RouterCache {
                entries: HashMap::new(),
            })),
            ttl: Duration::from_secs(30),
        }
    }

    /// Invalidate all cached entries (call after model config changes).
    pub async fn invalidate(&self) {
        let mut cache = self.cache.write().await;
        cache.entries.clear();
    }
}

#[async_trait]
impl ModelResolver for DbModelRouter {
    async fn resolve(&self, operation: &str) -> OxResult<ResolvedModel> {
        let ws_id = WORKSPACE_ID.try_with(|id| *id).ok();
        let cache_key = (ws_id, operation.to_string());

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some((model, inserted)) = cache.entries.get(&cache_key)
                && inserted.elapsed() < self.ttl
            {
                return Ok(model.clone());
            }
        }

        // Single SQL query handles the full 4-level priority chain:
        // workspace+operation > workspace+wildcard > global+operation > global+wildcard
        // Uses system_bypass so model configs are always readable regardless of RLS context.
        let config = ox_store::PostgresStore::with_system_bypass(|| {
            self.store.find_model_for_operation(operation, ws_id)
        })
        .await?;

        let Some(config) = config else {
            if let Some(fallback) = &self.fallback {
                return fallback.resolve(operation).await;
            }
            return Err(OxError::Runtime {
                message: format!(
                    "No model configured for operation '{operation}'. \
                     Add a model config with a '*' routing rule as fallback."
                ),
            });
        };

        let resolved = ResolvedModel {
            provider: config.provider.clone(),
            model_id: config.model_id.clone(),
            max_tokens: Some(config.max_tokens as u32),
            temperature: config.temperature,
            provider_config: Some(LlmProviderConfig {
                provider: config.provider,
                model: config.model_id,
                api_key: config
                    .api_key_env
                    .as_deref()
                    .and_then(|name| std::env::var(name).ok()),
                region: config.region,
                base_url: config.base_url,
                timeout_secs: Some(config.timeout_secs.max(1) as u64),
            }),
        };

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache
                .entries
                .insert(cache_key, (resolved.clone(), Instant::now()));
        }

        Ok(resolved)
    }
}

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use dashmap::DashMap;
use tracing::info;

use ox_core::error::{OxError, OxResult};

use crate::auth::LlmProviderConfig;

// ---------------------------------------------------------------------------
// ClientPool — shared branchforge Client pool keyed by provider identity
// ---------------------------------------------------------------------------

/// Pool of branchforge Clients keyed by provider identity.
///
/// Clients are keyed by (provider, api_key, base_url, region) — the credential
/// identity. Model is NOT part of the key because the same client can serve
/// different models from the same provider.
///
/// For model-specific operations, callers pass the model name to
/// `structured_completion` directly — the client just handles auth/transport.
pub struct ClientPool {
    /// Key: provider identity hash. Value: client.
    clients: DashMap<u64, PoolEntry>,
    /// Cached credentials for Agent auth (Auth::Resolved).
    credentials: DashMap<u64, branchforge::Credential>,
}

struct PoolEntry {
    client: Arc<branchforge::Client>,
    provider: String,
}

impl Default for ClientPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientPool {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            credentials: DashMap::new(),
        }
    }

    /// Get or create a Client for the given provider config.
    ///
    /// Clients are cached by provider identity (provider + api_key + base_url + region).
    /// The `model` field determines the default model reported by the adapter,
    /// but callers can override the model per-request via CreateMessageRequest.
    pub async fn get_or_create(
        &self,
        config: &LlmProviderConfig,
    ) -> OxResult<Arc<branchforge::Client>> {
        let key = provider_identity_hash(config);

        if let Some(entry) = self.clients.get(&key) {
            return Ok(Arc::clone(&entry.client));
        }

        let auth = config.resolve_auth()?;

        // Resolve and cache credential
        let credential = auth.resolve().await.map_err(|e| OxError::Runtime {
            message: format!("Credential resolution failed: {e}"),
        })?;
        self.credentials.insert(key, credential);

        let mut builder = branchforge::Client::builder()
            .auth(auth)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Client auth failed: {e}"),
            })?;

        builder =
            builder.models(branchforge::client::ModelConfig::default().primary(&config.model));

        let client = builder.build().await.map_err(|e| OxError::Runtime {
            message: format!("Client build failed: {e}"),
        })?;

        info!(
            provider = %config.provider,
            model = %config.model,
            "LLM client created in pool"
        );

        let arc = Arc::new(client);
        self.clients.insert(
            key,
            PoolEntry {
                client: Arc::clone(&arc),
                provider: config.provider.clone(),
            },
        );
        Ok(arc)
    }

    /// Get a cached Client by provider name.
    ///
    /// If multiple clients exist for the same provider (different credentials),
    /// returns the first match. This is safe because `resolve_for_operation`
    /// only needs transport-level access — the model is specified per-request.
    pub fn get_by_provider(&self, provider: &str) -> Option<Arc<branchforge::Client>> {
        for entry in self.clients.iter() {
            if entry.value().provider == provider {
                return Some(Arc::clone(&entry.value().client));
            }
        }
        None
    }

    /// Return a pre-resolved `Auth::Resolved` for zero-cost agent auth.
    pub async fn resolved_auth(&self, config: &LlmProviderConfig) -> OxResult<branchforge::Auth> {
        let key = provider_identity_hash(config);

        if let Some(cred) = self.credentials.get(&key) {
            return Ok(branchforge::Auth::resolved(cred.clone()));
        }

        // Ensure client is created (which caches the credential)
        self.get_or_create(config).await?;

        let cred = self.credentials.get(&key).ok_or_else(|| OxError::Runtime {
            message: "Credential not found after client creation".to_string(),
        })?;
        Ok(branchforge::Auth::resolved(cred.clone()))
    }

    /// Invalidate all cached clients and credentials.
    pub fn invalidate_all(&self) {
        self.clients.clear();
        self.credentials.clear();
        info!("Client pool invalidated");
    }

    /// Invalidate a specific provider config.
    pub fn invalidate(&self, config: &LlmProviderConfig) {
        let key = provider_identity_hash(config);
        self.clients.remove(&key);
        self.credentials.remove(&key);
    }
}

/// Hash of provider identity fields — credentials that determine the connection.
/// Model is NOT included because the same auth works for all models from one provider.
fn provider_identity_hash(config: &LlmProviderConfig) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    config.provider.hash(&mut hasher);
    config.api_key.hash(&mut hasher);
    config.base_url.hash(&mut hasher);
    config.region.hash(&mut hasher);
    hasher.finish()
}

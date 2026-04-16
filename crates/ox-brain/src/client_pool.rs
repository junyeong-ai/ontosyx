use std::hash::{Hash, Hasher};
use std::sync::Arc;

use dashmap::DashMap;
use tracing::info;

use branchforge::{Credential, LlmCall, LlmClient};
use ox_core::error::{OxError, OxResult};

use crate::auth::LlmProviderConfig;

// ---------------------------------------------------------------------------
// ClientPool — shared branchforge LlmCall pool keyed by provider identity
// ---------------------------------------------------------------------------

/// Pool of branchforge LlmCall clients keyed by provider identity.
///
/// Uses [`LlmClient::from_auth`] which handles all provider-specific
/// transport, codec, and credential configuration (API key, OAuth, SigV4)
/// with automatic retry — zero provider-specific logic in this module.
pub struct ClientPool {
    /// Key: provider identity hash. Value: LlmCall client + metadata.
    clients: DashMap<u64, PoolEntry>,
    /// Cached credentials for Agent auth (Auth::Resolved).
    credentials: DashMap<u64, Credential>,
}

struct PoolEntry {
    client: Arc<dyn LlmCall>,
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

    /// Get or create an LlmCall client for the given provider config.
    ///
    /// Delegates to branchforge's [`LlmClient::from_auth`] which handles
    /// all provider types (API key, OAuth/ClaudeCli, Bedrock SigV4, etc.)
    /// with automatic retry.
    pub async fn get_or_create(
        &self,
        config: &LlmProviderConfig,
    ) -> OxResult<Arc<dyn LlmCall>> {
        let key = provider_identity_hash(config);

        if let Some(entry) = self.clients.get(&key) {
            return Ok(Arc::clone(&entry.client));
        }

        let auth = config.resolve_auth()?;

        // Cache credential for agent auth (Auth::Resolved zero-cost path).
        let credential = auth.clone().resolve().await.map_err(|e| OxError::Runtime {
            message: format!("Credential resolution failed: {e}"),
        })?;
        self.credentials.insert(key, credential);

        // Build client via branchforge's LlmClient — handles all provider-specific
        // transport/codec/credential logic with automatic retry.
        let client = LlmClient::from_auth(auth)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("LLM client build failed for '{}': {e}", config.provider),
            })?;

        info!(
            provider = %config.provider,
            model = %config.model,
            "LLM client created in pool"
        );

        self.clients.insert(
            key,
            PoolEntry {
                client: Arc::clone(&client),
                provider: config.provider.clone(),
            },
        );
        Ok(client)
    }

    /// Return a cached LlmCall client by provider name.
    pub fn by_provider(&self, provider: &str) -> Option<Arc<dyn LlmCall>> {
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
fn provider_identity_hash(config: &LlmProviderConfig) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    config.provider.hash(&mut hasher);
    config.api_key.hash(&mut hasher);
    config.base_url.hash(&mut hasher);
    config.region.hash(&mut hasher);
    hasher.finish()
}

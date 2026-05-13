//! `ChatModelRegistry` — provider-identity-keyed cache of
//! [`ChatRunnable`] handles.
//!
//! Key shape is `(provider, credential, region/base_url, model)` — two
//! requests with identical tuples share one [`ChatRunnable`] handle;
//! a different model name produces a separate entry. The cost of a
//! fresh entry is `ChatModel::clone()` plus a model-name string,
//! because entelix already pools the underlying `reqwest::Client` per
//! transport beneath the handle.
//!
//! API key participates in the key as a hash fingerprint — rotated
//! keys produce a fresh entry, the prior entry ages out via
//! [`Self::invalidate_idle`]. The raw credential never enters the
//! map's hash domain.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use entelix::PolicyRegistry;
use tracing::info;

use ox_core::error::OxResult;

use crate::auth::LlmProviderConfig;
use crate::chat_model::ChatRunnable;
use crate::chat_model_factory::build_chat_model;

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Composite key — the four-axis identity that determines whether two
/// configs share a [`ChatRunnable`] handle. Distinct models on the
/// same credentials get separate entries; the underlying
/// transport+codec sharing is handled below the registry, inside
/// entelix's per-transport `reqwest::Client` pool.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct IdentityKey {
    provider: String,
    api_key_fingerprint: u64,
    base_url: Option<String>,
    region: Option<String>,
    model: String,
}

impl IdentityKey {
    fn from_config(config: &LlmProviderConfig) -> Self {
        // Key the api_key by hash, not by literal value, so the
        // DashMap key never carries the raw credential. Two identical
        // keys produce identical hashes; rotated keys produce a
        // miss + fresh build.
        let api_key_fingerprint = if let Some(key) = &config.api_key {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            hasher.finish()
        } else {
            0
        };
        Self {
            provider: config.provider.clone(),
            api_key_fingerprint,
            base_url: config.base_url.clone(),
            region: config.region.clone(),
            model: config.model.clone(),
        }
    }
}

struct Entry {
    handle: ChatRunnable,
    provider: String,
    last_used: AtomicU64,
}

/// Provider-identity-keyed cache of [`ChatRunnable`] handles.
///
/// Cheap to clone — internally `Arc<DashMap<...>>`. Operators wire one
/// registry per process and share `Arc<ChatModelRegistry>` across
/// every Brain consumer.
///
/// When [`Self::with_policy_registry`] is wired, every freshly-built
/// handle has `entelix::PolicyLayer` applied at construction.
/// The layer carries the tenant-scoped policy stack (PII redactor,
/// quota gate, cost meter) — `RunBudget`'s token and cost axes are
/// pre-checked before the wire roundtrip, and the cost ledger
/// charges on the `Ok` branch.
#[derive(Default)]
pub struct ChatModelRegistry {
    entries: DashMap<IdentityKey, Entry>,
    policy: Option<Arc<PolicyRegistry>>,
}

impl ChatModelRegistry {
    /// Empty registry. Lazy — entries materialise on the first
    /// matching [`Self::get_or_build`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach an [`entelix::PolicyRegistry`] so every
    /// freshly-built `ChatModel` has `PolicyLayer` applied at
    /// construction. The layer handles RunBudget pre-call gates
    /// (token + cost), per-tenant cost ledger charging on the `Ok`
    /// branch, optional PII redaction, and quota enforcement —
    /// uniformly across Brain helpers, the agent loop, and direct
    /// `ChatRunnable` consumers.
    #[must_use]
    pub fn with_policy_registry(mut self, policy: Arc<PolicyRegistry>) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Return a handle for `config`, building one through
    /// [`build_chat_model`] when no cached match exists.
    pub async fn get_or_build(&self, config: &LlmProviderConfig) -> OxResult<ChatRunnable> {
        let key = IdentityKey::from_config(config);

        if let Some(entry) = self.entries.get(&key) {
            entry.last_used.store(now_epoch_secs(), Ordering::Relaxed);
            return Ok(entry.handle.clone());
        }

        // Miss — build outside the DashMap lock to keep the critical
        // section short. A concurrent caller may also miss and build;
        // the DashMap entry API ensures only one survives the insert.
        let handle = build_chat_model(config, self.policy.as_ref()).await?;
        let entry = Entry {
            handle: handle.clone(),
            provider: config.provider.clone(),
            last_used: AtomicU64::new(now_epoch_secs()),
        };
        let stored = self.entries.entry(key).or_insert(entry);
        info!(
            provider = %config.provider,
            model = %config.model,
            "chat model handle cached"
        );
        Ok(stored.handle.clone())
    }

    /// Drop entries idle for at least `max_idle_secs`. Background
    /// schedulers call this periodically (every few minutes is
    /// plenty); single-tenant deployments without rotated credentials
    /// can skip it entirely.
    pub fn invalidate_idle(&self, max_idle_secs: u64) {
        let cutoff = now_epoch_secs().saturating_sub(max_idle_secs);
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            let keep = entry.last_used.load(Ordering::Relaxed) > cutoff;
            if !keep {
                info!(provider = %entry.provider, "chat model handle evicted (idle)");
            }
            keep
        });
        let evicted = before.saturating_sub(self.entries.len());
        if evicted > 0 {
            info!(
                evicted,
                remaining = self.entries.len(),
                "chat model registry idle eviction complete"
            );
        }
    }

    /// Drop every cached handle. Used by admin endpoints after a
    /// model-config update so the next request rebuilds against the
    /// new shape (rotated key, swapped region, etc.).
    pub fn invalidate_all(&self) {
        self.entries.clear();
        info!("chat model registry cleared");
    }

    /// Drop the entry matching `config`. Targeted invalidation — the
    /// admin-API model-update path uses this when only one provider
    /// changes.
    pub fn invalidate(&self, config: &LlmProviderConfig) {
        let key = IdentityKey::from_config(config);
        self.entries.remove(&key);
    }

    /// Number of cached handles. Diagnostic — production metrics
    /// endpoints surface this for capacity planning.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry holds zero handles. Diagnostic.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_model::{ChatRunnable, DynChatModel};
    use async_trait::async_trait;
    use entelix::ExecutionContext;
    use entelix::ir::{ContentPart, Message, ModelRequest, ModelResponse, StopReason, Usage};

    struct StubChatModel;

    #[async_trait]
    impl DynChatModel for StubChatModel {
        async fn complete_request(
            &self,
            _request: ModelRequest,
            _ctx: &ExecutionContext,
        ) -> entelix::Result<ModelResponse> {
            Ok(ModelResponse {
                id: "stub".into(),
                model: "stub".into(),
                stop_reason: StopReason::EndTurn,
                content: vec![ContentPart::Text {
                    text: String::new(),
                    cache_control: None,
                    provider_echoes: Vec::new(),
                }],
                usage: Usage::default(),
                rate_limit: None,
                warnings: Vec::new(),
                provider_echoes: Vec::new(),
            })
        }

        fn build_request(&self, messages: Vec<Message>) -> ModelRequest {
            ModelRequest {
                model: "stub".into(),
                messages,
                ..ModelRequest::default()
            }
        }
    }

    fn insert_stub(registry: &ChatModelRegistry, provider: &str, model: &str, age_secs: u64) {
        let entry = Entry {
            handle: ChatRunnable::new(StubChatModel),
            provider: provider.to_string(),
            last_used: AtomicU64::new(now_epoch_secs().saturating_sub(age_secs)),
        };
        let key = IdentityKey {
            provider: provider.to_string(),
            api_key_fingerprint: 0,
            base_url: None,
            region: None,
            model: model.to_string(),
        };
        registry.entries.insert(key, entry);
    }

    #[test]
    fn invalidate_idle_evicts_only_stale_entries() {
        let registry = ChatModelRegistry::new();
        insert_stub(&registry, "anthropic", "fresh", 0);
        insert_stub(&registry, "anthropic", "stale", 600);
        assert_eq!(registry.len(), 2);

        registry.invalidate_idle(60);

        assert_eq!(registry.len(), 1);
        // The fresh entry survives — its `last_used` is at "now",
        // beyond the 60-second cutoff.
        let surviving = registry
            .entries
            .iter()
            .map(|e| e.key().model.clone())
            .collect::<Vec<_>>();
        assert_eq!(surviving, vec!["fresh"]);
    }

    #[test]
    fn invalidate_idle_zero_window_drops_everything() {
        let registry = ChatModelRegistry::new();
        insert_stub(&registry, "anthropic", "a", 0);
        insert_stub(&registry, "openai", "b", 0);

        // Cutoff = now - 0 = now; nothing is strictly greater than now.
        registry.invalidate_idle(0);

        assert!(registry.is_empty());
    }

    #[test]
    fn invalidate_idle_noop_when_window_exceeds_age() {
        let registry = ChatModelRegistry::new();
        insert_stub(&registry, "anthropic", "a", 30);
        insert_stub(&registry, "openai", "b", 30);

        registry.invalidate_idle(3600);

        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn invalidate_all_drops_every_entry() {
        let registry = ChatModelRegistry::new();
        insert_stub(&registry, "anthropic", "a", 0);
        insert_stub(&registry, "openai", "b", 0);

        registry.invalidate_all();

        assert!(registry.is_empty());
    }
}

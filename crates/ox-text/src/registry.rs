//! Per-workspace tokenizer registry — lazy load + LRU eviction
//! + ArcSwap hot-swap on glossary publish.
//!
//! ## Invariants
//!
//! - **One Arc per workspace**. Index path + query path resolve
//!   the same `Arc<dyn Tokenizer>` for `(workspace_id)` —
//!   recall consistency by construction.
//! - **Hot-swap** via `ArcSwap`. `commit_version` publishes a
//!   new tokenizer; readers in flight on the old one finish
//!   safely; readers after the publish see the new one without
//!   restart.
//! - **Lazy load**. First lookup for a workspace builds the
//!   tokenizer (system dict shared, optional user dict from
//!   the workspace's glossary). Cold workspaces pay nothing.
//! - **LRU eviction**. Cap on resident workspaces; cold
//!   workspaces evict to keep memory bounded under enterprise
//!   tenant counts (1000+ workspaces). System dict is shared
//!   across all entries — only user dicts are per-workspace.
//!
//! ## Failure mode
//!
//! User-dict build failure preserves the last-known-good
//! tokenizer for the workspace and surfaces a typed error to
//! the caller (commit_version logs warn + alert hook).
//! Retrieval continues with the prior tokenizer rather than
//! falling back to system-only — sudden recall regression on
//! every other surface would be operator-confusing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwapAny;
use dashmap::DashMap;
use thiserror::Error;
use uuid::Uuid;

use crate::tokenizer::{KoreanEnglishTokenizer, TokenizeError, Tokenizer};

/// `ArcSwap` 가 sized 만 받기 때문에, 우리의 `dyn Tokenizer`
/// trait object 는 `Box` 한 layer 에 packed 후 `Arc` 로
/// share. 외부 surface 는 여전히 `Arc<dyn Tokenizer>` —
/// internal box 는 implementation detail.
type BoxedTokenizer = Box<dyn Tokenizer>;
type TokenizerHandle = Arc<BoxedTokenizer>;
type TokenizerSwap = ArcSwapAny<TokenizerHandle>;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("tokenizer build failed: {0}")]
    Build(#[from] TokenizeError),
    #[error("user-dictionary CSV parse failed: {0}")]
    UserDictParse(String),
}

/// Registry tuning. The defaults fit a hundreds-of-workspaces
/// deployment; high-cardinality tenants (10k+) tighten the
/// cap.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Max workspaces with resident tokenizers. Beyond this,
    /// LRU eviction reclaims the coldest. Default 256 — large
    /// enough that most deployments are effectively eager,
    /// small enough that 10k-workspace tenants don't blow
    /// memory. Each tokenizer holds an `Arc` on the system
    /// dict (process-shared), so per-entry residual is just
    /// the user dict (~10KB-1MB).
    pub max_resident: usize,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self { max_resident: 256 }
    }
}

/// Per-workspace tokenizer registry.
///
/// Cheap to clone (single Arc on the inner shared state).
#[derive(Clone)]
pub struct WorkspaceTokenizerRegistry {
    inner: Arc<RegistryInner>,
}

struct RegistryInner {
    config: RegistryConfig,
    /// Per-workspace tokenizer + lock-free LRU touch counter.
    entries: DashMap<Uuid, EntryHandle>,
    /// Shared fallback tokenizer — system dict only, user dict
    /// empty. Returned by [`WorkspaceTokenizerRegistry::for_workspace`]
    /// for workspaces that haven't been published yet
    /// (cold-start).
    system_only: TokenizerHandle,
    /// Strictly-monotonic touch counter. Every `for_workspace` /
    /// `publish` bumps this and stamps the entry's `last_touched`.
    /// Eviction sorts ascending — smallest = coldest. AtomicU64
    /// removes the `Mutex<Instant>` lock + poisoning surface that
    /// the prior shape carried for what is logically a one-word
    /// write under no contention.
    tick: AtomicU64,
}

struct EntryHandle {
    tokenizer: Arc<TokenizerSwap>,
    /// Last-touch sequence number from `RegistryInner::tick`.
    /// Larger = more recent. Lock-free read/write.
    last_touched: AtomicU64,
}

impl WorkspaceTokenizerRegistry {
    /// Construct an empty registry. Tokenizers are loaded
    /// lazily as workspaces hit retrieval paths, then published
    /// (with their user dict) by the commit-path hook.
    pub fn new(config: RegistryConfig) -> Result<Self, RegistryError> {
        let system_only: TokenizerHandle =
            Arc::new(Box::new(KoreanEnglishTokenizer::system_only()?) as BoxedTokenizer);
        Ok(Self {
            inner: Arc::new(RegistryInner {
                config,
                entries: DashMap::new(),
                system_only,
                tick: AtomicU64::new(0),
            }),
        })
    }

    /// Build a registry seeded with a custom default
    /// tokenizer — used in tests + future multi-language
    /// workspaces that want a non-Korean default.
    pub fn with_default<T: Tokenizer + 'static>(config: RegistryConfig, system_only: T) -> Self {
        let system_only: TokenizerHandle = Arc::new(Box::new(system_only) as BoxedTokenizer);
        Self {
            inner: Arc::new(RegistryInner {
                config,
                entries: DashMap::new(),
                system_only,
                tick: AtomicU64::new(0),
            }),
        }
    }

    fn next_tick(&self) -> u64 {
        self.inner.tick.fetch_add(1, Ordering::Relaxed)
    }

    /// Resolve the active tokenizer for a workspace. When the
    /// workspace hasn't been published yet, returns the shared
    /// system-only tokenizer — every retrieval path always
    /// gets *something* back.
    pub fn for_workspace(&self, workspace_id: Uuid) -> TokenizerHandle {
        if let Some(entry) = self.inner.entries.get(&workspace_id) {
            entry
                .last_touched
                .store(self.next_tick(), Ordering::Relaxed);
            return entry.tokenizer.load_full();
        }
        Arc::clone(&self.inner.system_only)
    }

    /// Publish a freshly-built tokenizer for the workspace.
    /// Hot-swaps the entry's `ArcSwap` so in-flight readers on
    /// the old tokenizer finish safely; subsequent readers see
    /// the new one. Triggers LRU eviction if the entry count
    /// exceeds [`RegistryConfig::max_resident`].
    pub fn publish<T: Tokenizer + 'static>(&self, workspace_id: Uuid, tokenizer: T) {
        let arc_tok: TokenizerHandle = Arc::new(Box::new(tokenizer) as BoxedTokenizer);
        let tick = self.next_tick();
        match self.inner.entries.entry(workspace_id) {
            dashmap::Entry::Occupied(slot) => {
                slot.get().tokenizer.store(arc_tok);
                slot.get().last_touched.store(tick, Ordering::Relaxed);
            }
            dashmap::Entry::Vacant(slot) => {
                slot.insert(EntryHandle {
                    tokenizer: Arc::new(TokenizerSwap::new(arc_tok)),
                    last_touched: AtomicU64::new(tick),
                });
            }
        }
        self.evict_cold_if_over_cap();
    }

    /// Evict a workspace's tokenizer (e.g. on workspace
    /// deletion). Idempotent on missing entries.
    pub fn evict(&self, workspace_id: Uuid) {
        self.inner.entries.remove(&workspace_id);
    }

    /// Number of resident tokenizers (excluding the shared
    /// system-only fallback). Surfaces in observability for
    /// LRU tuning.
    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }

    /// Drop the coldest entries until the resident count is
    /// within the cap. Called after every `publish`. Bounded
    /// O(N log N) on the whole entry set — fine at our scale.
    fn evict_cold_if_over_cap(&self) {
        let cap = self.inner.config.max_resident;
        if cap == 0 {
            return;
        }
        let count = self.inner.entries.len();
        if count <= cap {
            return;
        }
        let to_evict = count - cap;

        let mut by_age: Vec<(Uuid, u64)> = self
            .inner
            .entries
            .iter()
            .map(|kv| (*kv.key(), kv.value().last_touched.load(Ordering::Relaxed)))
            .collect();
        by_age.sort_by_key(|(_, touched)| *touched);
        for (id, _) in by_age.into_iter().take(to_evict) {
            self.inner.entries.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::PassthroughTokenizer;

    #[test]
    fn unknown_workspace_returns_system_default() {
        let reg = WorkspaceTokenizerRegistry::with_default(
            RegistryConfig::default(),
            PassthroughTokenizer,
        );
        let ws = Uuid::new_v4();
        let t = reg.for_workspace(ws);
        assert_eq!(t.name(), "passthrough");
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn publish_then_lookup_returns_published_tokenizer() {
        let reg = WorkspaceTokenizerRegistry::with_default(
            RegistryConfig::default(),
            PassthroughTokenizer,
        );
        let ws = Uuid::new_v4();
        reg.publish(ws, PassthroughTokenizer);
        let t = reg.for_workspace(ws);
        assert_eq!(t.name(), "passthrough");
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn lru_evicts_cold_when_over_cap() {
        let reg = WorkspaceTokenizerRegistry::with_default(
            RegistryConfig { max_resident: 2 },
            PassthroughTokenizer,
        );
        let ws_a = Uuid::new_v4();
        let ws_b = Uuid::new_v4();
        let ws_c = Uuid::new_v4();
        reg.publish(ws_a, PassthroughTokenizer);
        reg.publish(ws_b, PassthroughTokenizer);
        // Touch ws_a so it's not the coldest — strictly-monotonic
        // tick counter promotes it past ws_b.
        let _ = reg.for_workspace(ws_a);
        reg.publish(ws_c, PassthroughTokenizer);
        // ws_b 가 coldest → evicted.
        assert_eq!(reg.len(), 2);
        assert!(reg.inner.entries.contains_key(&ws_a));
        assert!(reg.inner.entries.contains_key(&ws_c));
        assert!(!reg.inner.entries.contains_key(&ws_b));
    }

    #[test]
    fn republish_hot_swaps_in_place() {
        let reg = WorkspaceTokenizerRegistry::with_default(
            RegistryConfig::default(),
            PassthroughTokenizer,
        );
        let ws = Uuid::new_v4();
        reg.publish(ws, PassthroughTokenizer);
        let count_before = reg.len();
        reg.publish(ws, PassthroughTokenizer);
        assert_eq!(reg.len(), count_before, "republish should not grow count");
    }

    #[test]
    fn evict_removes_entry() {
        let reg = WorkspaceTokenizerRegistry::with_default(
            RegistryConfig::default(),
            PassthroughTokenizer,
        );
        let ws = Uuid::new_v4();
        reg.publish(ws, PassthroughTokenizer);
        assert_eq!(reg.len(), 1);
        reg.evict(ws);
        assert_eq!(reg.len(), 0);
    }
}

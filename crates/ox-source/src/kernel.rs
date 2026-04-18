//! Cross-cutting orchestration layer for [`DataSourceAdapter`] instances.
//!
//! Every adapter today re-implements the same concerns — retry on a
//! transient connection blip, optionally cache the last successful analysis,
//! surface structured warnings to callers — with slightly different shapes.
//! The [`IntrospectionKernel`] owns those concerns once so every adapter can
//! stay focused on its per-backend primitives.
//!
//! The kernel is a thin wrapper around an adapter. It never reinterprets
//! per-adapter semantics — if an adapter returns a [`ox_core::error::OxError`]
//! the caller decides what the error means. The kernel only adds:
//!
//! - **Retry.** Invoke the adapter under a [`RetryPolicy`] that classifies an
//!   error as transient (retryable) or permanent (fail-fast). A single
//!   connection hiccup in the middle of a PostgreSQL introspection no longer
//!   requires every adapter to embed its own backoff loop.
//! - **Schema cache.** Memoize the result of [`analyze`](IntrospectionKernel::analyze)
//!   with a TTL. Repeated calls within the window skip the adapter entirely —
//!   important when a UI re-requests analysis as the user navigates away and
//!   back, or a downstream caller wants to inspect warnings multiple times.
//! - **Warning aggregation.** The adapter's [`crate::AnalysisResult`] already
//!   carries [`ox_core::source_analysis::AnalysisWarning`]s; the kernel keeps
//!   them associated with the cached result so every consumer sees the same
//!   partial-analysis picture.
//!
//! Concurrency (running per-table introspection in parallel) still lives
//! inside each adapter — moving it to the kernel is a follow-up refactor
//! that requires switching adapters to atomic primitives.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use ox_core::error::{OxError, OxResult};

use crate::{AnalysisResult, DataSourceAdapter};

// ---------------------------------------------------------------------------
// RetryPolicy
// ---------------------------------------------------------------------------

/// Classifier deciding whether a given error is worth retrying.
///
/// Defaults to "no error is retryable" — so a `RetryPolicy` with its
/// default predicate acts as a single-attempt wrapper. Callers supply a
/// backend-aware predicate (for example one that recognises
/// `connection refused`, `timeout`, or a specific SQLSTATE) via
/// [`RetryPolicy::with_transient`].
pub type TransientPredicate = Arc<dyn Fn(&OxError) -> bool + Send + Sync>;

/// Configure how the kernel reacts to adapter errors.
///
/// A retry policy never turns an error into success — it just makes the
/// kernel call the adapter again. If every attempt fails, the last error
/// propagates to the caller verbatim.
#[derive(Clone)]
pub struct RetryPolicy {
    /// Total attempts including the first. `1` means "no retry".
    pub max_attempts: u32,
    /// Delay before the second attempt. Subsequent delays grow
    /// geometrically by `backoff_multiplier`.
    pub initial_backoff: Duration,
    /// Multiplier applied to the backoff between attempts. `2.0` is the
    /// textbook exponential default; `1.0` disables growth.
    pub backoff_multiplier: f64,
    /// Predicate deciding whether a given error is transient. A retry
    /// only happens when this returns `true`; permanent errors (syntax,
    /// authentication, validation) fail-fast.
    pub is_transient: TransientPredicate,
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("initial_backoff", &self.initial_backoff)
            .field("backoff_multiplier", &self.backoff_multiplier)
            .finish_non_exhaustive()
    }
}

impl RetryPolicy {
    /// Single-attempt policy: call the adapter once, surface any error.
    /// Useful for adapters with in-process data (CSV, JSON) where a
    /// "transient" failure is impossible by construction.
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(0),
            backoff_multiplier: 1.0,
            is_transient: Arc::new(|_| false),
        }
    }

    /// A reasonable default for network-backed adapters: up to 3 attempts,
    /// 100ms initial backoff doubling each retry. Callers must still
    /// attach a backend-specific transient predicate via
    /// [`RetryPolicy::with_transient`] — without one, errors never retry.
    pub fn exponential_default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            is_transient: Arc::new(|_| false),
        }
    }

    /// Replace the transient classifier while preserving timing
    /// parameters. The closure is stored as-is; cheap to clone.
    pub fn with_transient<F>(mut self, f: F) -> Self
    where
        F: Fn(&OxError) -> bool + Send + Sync + 'static,
    {
        self.is_transient = Arc::new(f);
        self
    }

    /// Override the maximum number of attempts (total, including the first).
    pub fn with_max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = n.max(1);
        self
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::no_retry()
    }
}

// ---------------------------------------------------------------------------
// CacheTtl
// ---------------------------------------------------------------------------

/// How long a cached analysis stays fresh.
///
/// Cache hits return an `Arc<AnalysisResult>` pointing at the same
/// allocation as the original success — callers never pay for a second
/// adapter round-trip within the window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CacheTtl {
    /// Never cache. Every [`IntrospectionKernel::analyze`] call reaches
    /// the adapter.
    #[default]
    Disabled,
    /// Cache entries expire after this duration. `Duration::MAX`
    /// effectively means "forever — invalidate explicitly".
    Duration(Duration),
}

impl CacheTtl {
    fn is_fresh(self, inserted_at: Instant) -> bool {
        match self {
            CacheTtl::Disabled => false,
            CacheTtl::Duration(ttl) => inserted_at.elapsed() < ttl,
        }
    }
}

// ---------------------------------------------------------------------------
// IntrospectionKernel
// ---------------------------------------------------------------------------

struct CacheEntry {
    analysis: Arc<AnalysisResult>,
    inserted_at: Instant,
}

/// Wraps a [`DataSourceAdapter`] with retry, caching, and warning
/// aggregation. Use this in favour of calling adapter methods directly
/// so every data-source consumer in the platform gets the same
/// cross-cutting behaviour.
///
/// Construction is cheap (single heap alloc for the mutex). Cloning an
/// `Arc<IntrospectionKernel>` is the right way to share the same cache
/// across tasks.
pub struct IntrospectionKernel {
    adapter: Arc<dyn DataSourceAdapter>,
    retry: RetryPolicy,
    cache_ttl: CacheTtl,
    cache: Mutex<Option<CacheEntry>>,
}

impl std::fmt::Debug for IntrospectionKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntrospectionKernel")
            .field("source_type", &self.adapter.source_type())
            .field("retry", &self.retry)
            .field("cache_ttl", &self.cache_ttl)
            .finish_non_exhaustive()
    }
}

impl IntrospectionKernel {
    /// Wrap an adapter with default (no-retry, no-cache) behaviour.
    /// Fluent builders (`with_retry`, `with_cache_ttl`) layer on the
    /// desired policies.
    pub fn new(adapter: Arc<dyn DataSourceAdapter>) -> Self {
        Self {
            adapter,
            retry: RetryPolicy::default(),
            cache_ttl: CacheTtl::default(),
            cache: Mutex::new(None),
        }
    }

    /// Attach a retry policy. Replaces any previously set policy.
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Configure cache TTL. Call with `CacheTtl::Disabled` to turn off
    /// caching entirely.
    pub fn with_cache_ttl(mut self, ttl: CacheTtl) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Reference to the wrapped adapter. Useful for code paths that
    /// need the `source_type()` without going through the kernel.
    pub fn adapter(&self) -> &dyn DataSourceAdapter {
        self.adapter.as_ref()
    }

    /// Source type of the wrapped adapter.
    pub fn source_type(&self) -> &str {
        self.adapter.source_type()
    }

    /// Drop any cached analysis. Next [`analyze`](Self::analyze) will
    /// always hit the adapter.
    pub async fn invalidate(&self) {
        *self.cache.lock().await = None;
    }

    /// Whether a fresh cached analysis is currently available. Intended
    /// for diagnostics and tests — production code should just call
    /// `analyze()` and rely on the cache to do its job transparently.
    pub async fn is_cached(&self) -> bool {
        let guard = self.cache.lock().await;
        guard
            .as_ref()
            .is_some_and(|e| self.cache_ttl.is_fresh(e.inserted_at))
    }

    /// Run full analysis through retry + cache. Identical to
    /// [`DataSourceAdapter::analyze`] except:
    ///
    /// - Transient errors retry according to the configured policy.
    /// - Repeat calls within the cache TTL reuse the previous successful
    ///   result (same `Arc`).
    ///
    /// Returns `Arc<AnalysisResult>` so sharing the warning list with
    /// multiple consumers doesn't clone the schema.
    pub async fn analyze(&self) -> OxResult<Arc<AnalysisResult>> {
        if let Some(cached) = self.cache_hit().await {
            return Ok(cached);
        }

        let result = self.run_analyze_with_retry().await?;
        let arc = Arc::new(result);

        if matches!(self.cache_ttl, CacheTtl::Duration(_)) {
            let mut guard = self.cache.lock().await;
            *guard = Some(CacheEntry {
                analysis: Arc::clone(&arc),
                inserted_at: Instant::now(),
            });
        }

        Ok(arc)
    }

    async fn cache_hit(&self) -> Option<Arc<AnalysisResult>> {
        if matches!(self.cache_ttl, CacheTtl::Disabled) {
            return None;
        }
        let guard = self.cache.lock().await;
        guard.as_ref().and_then(|e| {
            if self.cache_ttl.is_fresh(e.inserted_at) {
                Some(Arc::clone(&e.analysis))
            } else {
                None
            }
        })
    }

    async fn run_analyze_with_retry(&self) -> OxResult<AnalysisResult> {
        let mut attempt: u32 = 0;
        let mut backoff = self.retry.initial_backoff;
        loop {
            attempt += 1;
            match self.adapter.analyze().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    let transient = (self.retry.is_transient)(&err);
                    let attempts_left = attempt < self.retry.max_attempts;
                    if !transient || !attempts_left {
                        return Err(err);
                    }
                    if backoff > Duration::from_millis(0) {
                        tokio::time::sleep(backoff).await;
                    }
                    backoff = scale_backoff(backoff, self.retry.backoff_multiplier);
                }
            }
        }
    }
}

/// Apply the retry policy's geometric growth, clamping to the backoff's
/// own precision. Keeps the math in one place so tests can exercise the
/// growth curve.
fn scale_backoff(current: Duration, multiplier: f64) -> Duration {
    let nanos = current.as_nanos() as f64 * multiplier;
    if nanos.is_finite() && nanos >= 0.0 {
        Duration::from_nanos(nanos.min(u64::MAX as f64) as u64)
    } else {
        current
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ox_core::source_schema::{SourceProfile, SourceSchema};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Adapter whose behaviour is fully scripted by a VecDeque of
    /// results. Each `analyze()` call consumes one entry. Useful for
    /// driving retry / cache scenarios deterministically.
    struct ScriptedAdapter {
        calls: AtomicUsize,
        responses: std::sync::Mutex<std::collections::VecDeque<OxResult<AnalysisResult>>>,
    }

    impl ScriptedAdapter {
        fn new(responses: Vec<OxResult<AnalysisResult>>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                responses: std::sync::Mutex::new(responses.into_iter().collect()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl DataSourceAdapter for ScriptedAdapter {
        fn source_type(&self) -> &str {
            "scripted"
        }
        async fn introspect_schema(&self) -> OxResult<SourceSchema> {
            unreachable!("kernel tests only exercise analyze()")
        }
        async fn collect_stats(&self, _schema: &SourceSchema) -> OxResult<SourceProfile> {
            unreachable!("kernel tests only exercise analyze()")
        }
        async fn analyze(&self) -> OxResult<AnalysisResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.responses
                .lock()
                .expect("mutex")
                .pop_front()
                .expect("ScriptedAdapter: no more scripted responses")
        }
    }

    fn empty_analysis() -> AnalysisResult {
        AnalysisResult {
            schema: SourceSchema {
                source_type: "scripted".to_string(),
                tables: Vec::new(),
                foreign_keys: Vec::new(),
            },
            profile: SourceProfile {
                table_profiles: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    fn transient_err() -> OxError {
        OxError::Runtime {
            message: "connection refused".to_string(),
        }
    }

    fn permanent_err() -> OxError {
        OxError::Validation {
            field: "query".to_string(),
            message: "syntax error".to_string(),
        }
    }

    // --- Retry behaviour -------------------------------------------------

    #[tokio::test]
    async fn retry_policy_retries_transient_errors() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Err(transient_err()),
            Err(transient_err()),
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_retry(RetryPolicy::exponential_default().with_transient(
                |e| matches!(e, OxError::Runtime { message } if message.contains("connection refused")),
            ));
        let result = kernel.analyze().await;
        assert!(result.is_ok(), "expected success after retries: {result:?}");
        assert_eq!(adapter.call_count(), 3);
    }

    #[tokio::test]
    async fn retry_policy_exhausts_attempts_on_persistent_transient() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Err(transient_err()),
            Err(transient_err()),
            Err(transient_err()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone()).with_retry(
            RetryPolicy::exponential_default()
                .with_transient(|e| matches!(e, OxError::Runtime { .. })),
        );
        let result = kernel.analyze().await;
        assert!(result.is_err(), "expected failure after exhausting retries");
        assert_eq!(adapter.call_count(), 3, "every attempt must be used");
    }

    #[tokio::test]
    async fn retry_policy_fail_fast_on_permanent_error() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![Err(permanent_err())]));
        let kernel = IntrospectionKernel::new(adapter.clone()).with_retry(
            RetryPolicy::exponential_default()
                .with_transient(|e| matches!(e, OxError::Runtime { .. })),
        );
        let result = kernel.analyze().await;
        assert!(result.is_err());
        assert_eq!(
            adapter.call_count(),
            1,
            "permanent error must fail on the first attempt"
        );
    }

    #[tokio::test]
    async fn default_retry_is_single_attempt() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![Err(transient_err())]));
        let kernel = IntrospectionKernel::new(adapter.clone());
        let result = kernel.analyze().await;
        assert!(result.is_err());
        assert_eq!(adapter.call_count(), 1);
    }

    #[tokio::test]
    async fn custom_max_attempts_honoured() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Err(transient_err()),
            Err(transient_err()),
            Err(transient_err()),
            Err(transient_err()),
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone()).with_retry(
            RetryPolicy::exponential_default()
                .with_max_attempts(5)
                .with_transient(|e| matches!(e, OxError::Runtime { .. })),
        );
        let result = kernel.analyze().await;
        assert!(result.is_ok());
        assert_eq!(adapter.call_count(), 5);
    }

    // --- Cache behaviour -------------------------------------------------

    #[tokio::test]
    async fn cache_disabled_always_hits_adapter() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Ok(empty_analysis()),
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone());
        let _ = kernel.analyze().await.unwrap();
        let _ = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 2);
    }

    #[tokio::test]
    async fn cache_hit_within_ttl_skips_adapter() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![Ok(empty_analysis())]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_cache_ttl(CacheTtl::Duration(Duration::from_secs(60)));
        let first = kernel.analyze().await.unwrap();
        let second = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 1, "second call must hit the cache");
        assert!(Arc::ptr_eq(&first, &second), "same Arc on cache hit");
    }

    #[tokio::test]
    async fn cache_miss_after_ttl_expiry_calls_adapter_again() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Ok(empty_analysis()),
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_cache_ttl(CacheTtl::Duration(Duration::from_millis(5)));
        let _ = kernel.analyze().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 2);
    }

    #[tokio::test]
    async fn invalidate_forces_next_call_to_adapter() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Ok(empty_analysis()),
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_cache_ttl(CacheTtl::Duration(Duration::from_secs(60)));
        let _ = kernel.analyze().await.unwrap();
        assert!(kernel.is_cached().await);
        kernel.invalidate().await;
        assert!(!kernel.is_cached().await);
        let _ = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 2);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Err(permanent_err()),
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_cache_ttl(CacheTtl::Duration(Duration::from_secs(60)));
        let first = kernel.analyze().await;
        assert!(first.is_err());
        assert!(!kernel.is_cached().await, "error must not populate cache");
        let second = kernel.analyze().await;
        assert!(second.is_ok());
        assert_eq!(adapter.call_count(), 2);
    }

    // --- Cache + retry interaction --------------------------------------

    #[tokio::test]
    async fn cache_stores_result_produced_by_retry() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![
            Err(transient_err()),
            Ok(empty_analysis()),
            // Extra entry never reached if cache works — asserts below.
            Ok(empty_analysis()),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_retry(
                RetryPolicy::exponential_default()
                    .with_transient(|e| matches!(e, OxError::Runtime { .. })),
            )
            .with_cache_ttl(CacheTtl::Duration(Duration::from_secs(60)));

        let first = kernel.analyze().await.unwrap();
        let second = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 2, "retry used 2 calls, cache blocks 3rd");
        assert!(Arc::ptr_eq(&first, &second));
    }

    // --- Misc API ---------------------------------------------------------

    #[tokio::test]
    async fn adapter_accessor_exposes_source_type() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![]));
        let kernel = IntrospectionKernel::new(adapter);
        assert_eq!(kernel.source_type(), "scripted");
        assert_eq!(kernel.adapter().source_type(), "scripted");
    }

    #[test]
    fn retry_policy_default_is_no_retry() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_attempts, 1);
    }

    #[test]
    fn cache_ttl_default_disabled() {
        assert_eq!(CacheTtl::default(), CacheTtl::Disabled);
    }

    #[test]
    fn cache_ttl_fresh_semantics() {
        let now = Instant::now();
        assert!(!CacheTtl::Disabled.is_fresh(now));
        assert!(CacheTtl::Duration(Duration::from_secs(60)).is_fresh(now));
    }

    #[test]
    fn scale_backoff_grows_geometrically() {
        let b = Duration::from_millis(100);
        let b2 = scale_backoff(b, 2.0);
        let b3 = scale_backoff(b2, 2.0);
        assert_eq!(b2, Duration::from_millis(200));
        assert_eq!(b3, Duration::from_millis(400));
    }

    #[test]
    fn scale_backoff_noop_on_nonfinite_multiplier() {
        // NaN and Infinity both fail `is_finite`, so the scaling short-circuits
        // to the unchanged backoff rather than running the multiplication.
        let b = Duration::from_millis(100);
        assert_eq!(scale_backoff(b, f64::NAN), b);
        assert_eq!(scale_backoff(b, f64::INFINITY), b);
    }

    #[test]
    fn with_max_attempts_clamps_to_one() {
        let p = RetryPolicy::exponential_default().with_max_attempts(0);
        assert_eq!(p.max_attempts, 1);
    }
}

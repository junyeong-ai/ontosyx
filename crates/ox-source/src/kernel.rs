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
//!   carries [`ox_ontology::source_analysis::AnalysisWarning`]s; the kernel keeps
//!   them associated with the cached result so every consumer sees the same
//!   partial-analysis picture.
//!
//! Concurrency (running per-table introspection in parallel) still lives
//! inside each adapter — moving it to the kernel is a follow-up refactor
//! that requires switching adapters to atomic primitives.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use tokio::sync::Mutex;
use tracing::warn;

use ox_core::error::{OxError, OxResult};
use ox_ontology::source_analysis::{
    AnalysisPhase, AnalysisWarning, AnalysisWarningKind, LARGE_SCHEMA_GATE_THRESHOLD, PiiDecision,
    PiiDecisionEntry, WarningLevel,
};
use ox_core::source_schema::{SourceProfile, SourceSchema, TableProfile};

use crate::{AnalysisResult, DEFAULT_INTROSPECTION_CONCURRENCY, DataSourceAdapter};

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

/// Wraps a [`DataSourceAdapter`] with retry, caching, concurrent
/// orchestration, and warning aggregation. Use this in favour of calling
/// adapter methods directly so every data-source consumer in the platform
/// gets the same cross-cutting behaviour.
///
/// Construction is cheap (single heap alloc for the mutex). Cloning an
/// `Arc<IntrospectionKernel>` is the right way to share the same cache
/// across tasks.
pub struct IntrospectionKernel {
    adapter: Arc<dyn DataSourceAdapter>,
    retry: RetryPolicy,
    cache_ttl: CacheTtl,
    concurrency: usize,
    cache: Mutex<Option<CacheEntry>>,
}

impl std::fmt::Debug for IntrospectionKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntrospectionKernel")
            .field("source_type", &self.adapter.source_type())
            .field("retry", &self.retry)
            .field("cache_ttl", &self.cache_ttl)
            .field("concurrency", &self.concurrency)
            .finish_non_exhaustive()
    }
}

impl IntrospectionKernel {
    /// Wrap an adapter with default (no-retry, no-cache, default
    /// concurrency) behaviour. Fluent builders
    /// (`with_retry` / `with_cache_ttl` / `with_concurrency`) layer on
    /// the desired policies.
    pub fn new(adapter: Arc<dyn DataSourceAdapter>) -> Self {
        Self {
            adapter,
            retry: RetryPolicy::default(),
            cache_ttl: CacheTtl::default(),
            concurrency: DEFAULT_INTROSPECTION_CONCURRENCY,
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

    /// Override the concurrent-primitive fan-out (default: 8). Kept for
    /// adapters with connection pools that can serve more/fewer parallel
    /// callers than the default.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.max(1);
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

    /// Run full analysis: schema + profile + warnings, under retry and
    /// cache policy.
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

    /// Discover the source's schema (tables + columns + PK + FKs), with
    /// per-table warnings captured rather than surfaced as fatal errors.
    /// Resilient: a table whose `describe_table` fails is skipped with
    /// a `TableSkipped` warning; FK discovery failures degrade to an
    /// empty FK set with a warning. Returns `Err` only when the source
    /// is fundamentally unreachable or every table is inaccessible.
    pub async fn introspect_schema(&self) -> OxResult<(SourceSchema, Vec<AnalysisWarning>)> {
        let table_names = self.adapter.list_tables().await?;
        if table_names.len() >= LARGE_SCHEMA_GATE_THRESHOLD {
            warn!(
                table_count = table_names.len(),
                threshold = LARGE_SCHEMA_GATE_THRESHOLD,
                "Large schema detected. Introspection may take significant time on the source.",
            );
        }

        let mut warnings = Vec::new();

        // Describe every table concurrently, preserving input order.
        let adapter = Arc::clone(&self.adapter);
        let describe_results: Vec<(String, OxResult<ox_core::source_schema::SourceTableDef>)> =
            run_concurrent(&table_names, self.concurrency, |name| {
                let adapter = Arc::clone(&adapter);
                async move {
                    let name_for_call = name.clone();
                    let result = adapter.describe_table(&name_for_call).await;
                    (name, result)
                }
            })
            .await;

        let mut tables = Vec::with_capacity(table_names.len());
        for (table_name, result) in describe_results {
            match result {
                Ok(t) => tables.push(t),
                Err(err) => {
                    warn!(table = %table_name, error = %err, "Skipping inaccessible table during schema introspection");
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::SchemaIntrospection,
                        kind: AnalysisWarningKind::TableSkipped,
                        location: table_name,
                        message: err.to_string(),
                    });
                }
            }
        }

        if tables.is_empty() && !table_names.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "No accessible tables were introspected from {}",
                    self.adapter.source_type()
                ),
            });
        }

        // Foreign keys: a discovery failure degrades to an empty set
        // with a single warning rather than failing the whole flow.
        let mut foreign_keys = match self.adapter.list_foreign_keys().await {
            Ok(fks) => fks,
            Err(err) => {
                warn!(error = %err, "Foreign key discovery failed; continuing without declared foreign keys");
                warnings.push(AnalysisWarning {
                    level: WarningLevel::Warning,
                    phase: AnalysisPhase::SchemaIntrospection,
                    kind: AnalysisWarningKind::ForeignKeysUnavailable,
                    location: self.adapter.source_type().to_string(),
                    message: err.to_string(),
                });
                Vec::new()
            }
        };

        // Filter FKs to tables that actually made it into the schema —
        // a dangling FK referring to a skipped table would just confuse
        // downstream graph-edge inference.
        let accessible: std::collections::HashSet<&str> =
            tables.iter().map(|t| t.name.as_str()).collect();
        foreign_keys.retain(|fk| {
            accessible.contains(fk.from_table.as_str()) && accessible.contains(fk.to_table.as_str())
        });

        Ok((
            SourceSchema {
                source_type: self.adapter.source_type().to_string(),
                tables,
                foreign_keys,
            },
            warnings,
        ))
    }

    /// Profile every table in `schema`: row count + per-column
    /// [`ColumnStats`](ox_core::source_schema::ColumnStats). Each
    /// failure converts to a warning rather than a fatal error so a
    /// single inaccessible table doesn't kill an otherwise useful
    /// analysis.
    pub async fn collect_stats(
        &self,
        schema: &SourceSchema,
    ) -> OxResult<(SourceProfile, Vec<AnalysisWarning>)> {
        let adapter = Arc::clone(&self.adapter);
        let concurrency = self.concurrency;

        let items: Vec<(String, Vec<ox_core::source_schema::SourceColumnDef>)> = schema
            .tables
            .iter()
            .map(|t| (t.name.clone(), t.columns.clone()))
            .collect();

        let profile_results = run_concurrent(&items, concurrency, |(name, columns)| {
            let adapter = Arc::clone(&adapter);
            async move {
                let result = profile_table(adapter.as_ref(), &name, &columns).await;
                (name, result)
            }
        })
        .await;

        let mut table_profiles = Vec::new();
        let mut warnings = Vec::new();
        for (table_name, result) in profile_results {
            match result {
                Ok((tp, mut tp_warnings)) => {
                    table_profiles.push(tp);
                    warnings.append(&mut tp_warnings);
                }
                Err(err) => {
                    warn!(table = %table_name, error = %err, "Skipping table during data profiling");
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::DataProfiling,
                        kind: AnalysisWarningKind::TableSkipped,
                        location: table_name,
                        message: err.to_string(),
                    });
                }
            }
        }

        if table_profiles.is_empty() && !schema.tables.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "Failed to collect stats for any table in {}",
                    self.adapter.source_type()
                ),
            });
        }

        Ok((SourceProfile { table_profiles }, warnings))
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
            match self.run_full_analysis_once().await {
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

    async fn run_full_analysis_once(&self) -> OxResult<AnalysisResult> {
        let (schema, mut warnings) = self.introspect_schema().await?;
        let (mut profile, profile_warnings) = self.collect_stats(&schema).await?;
        warnings.extend(profile_warnings);

        // Default PII redaction — every PII-flagged column has its
        // `sample_values` replaced with `[MASKED]` before the analysis
        // leaves the kernel. The caller (design UI / agent tool) can
        // surface `PiiDecision::Allow` on a per-column basis to fetch an
        // unmasked profile when a reviewer has explicitly signed off.
        //
        // Why this lives in the kernel: the PII detector already runs
        // during `build_analysis_report`, but that report is a separate
        // artefact from the profile. Raw sample values travel in the
        // profile. Masking here closes the gap — the profile returned
        // to downstream code (and, critically, serialised to the
        // frontend) never carries raw PII unless a decision override
        // says otherwise.
        let pii_findings = crate::analyzer::detect_pii(&schema, &profile);
        if !pii_findings.is_empty() {
            let default_mask: Vec<_> = pii_findings
                .iter()
                .map(|f| PiiDecisionEntry {
                    table: f.table.clone(),
                    column: f.column.clone(),
                    decision: PiiDecision::Mask,
                })
                .collect();
            crate::analyzer::apply_pii_masking(&mut profile, &default_mask);
        }

        Ok(AnalysisResult {
            schema,
            profile,
            warnings,
        })
    }
}

/// Profile one table against an adapter. Extracted as a free function so
/// the kernel can capture it inside a concurrent stream without
/// re-borrowing `self`. Column-level failures degrade to a warning +
/// skipped `ColumnStats` rather than a fatal table error.
async fn profile_table(
    adapter: &(dyn DataSourceAdapter + '_),
    table_name: &str,
    columns: &[ox_core::source_schema::SourceColumnDef],
) -> OxResult<(TableProfile, Vec<AnalysisWarning>)> {
    let row_count = adapter.count_rows(table_name).await?;
    let mut column_stats = Vec::new();
    let mut warnings = Vec::new();
    for col in columns {
        match adapter.sample_column(table_name, col).await {
            Ok(stats) => column_stats.push(stats),
            Err(err) => {
                warn!(
                    table = %table_name,
                    column = %col.name,
                    error = %err,
                    "Skipping column during data profiling"
                );
                warnings.push(AnalysisWarning {
                    level: WarningLevel::Warning,
                    phase: AnalysisPhase::DataProfiling,
                    kind: AnalysisWarningKind::ColumnSkipped,
                    location: format!("{table_name}.{}", col.name),
                    message: err.to_string(),
                });
            }
        }
    }
    Ok((
        TableProfile {
            table_name: table_name.to_string(),
            row_count,
            column_stats,
        },
        warnings,
    ))
}

/// Run an async closure over each item with bounded parallelism,
/// returning results in input order.
async fn run_concurrent<T, F, Fut, R>(items: &[T], concurrency: usize, f: F) -> Vec<R>
where
    T: Clone + Send + Sync,
    F: Fn(T) -> Fut + Send + Sync + Copy,
    Fut: std::future::Future<Output = R> + Send,
    R: Send,
{
    let mut results: Vec<(usize, R)> = stream::iter(items.iter().cloned().enumerate())
        .map(|(idx, item)| async move {
            let r = f(item).await;
            (idx, r)
        })
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    results.sort_by_key(|(idx, _)| *idx);
    results.into_iter().map(|(_, r)| r).collect()
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
    use ox_core::source_schema::{ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Adapter whose `list_tables()` return is scripted per-call.
    /// Every other primitive is a fixed empty response (kernel tests
    /// exercise retry / cache behaviour, not per-column profiling).
    /// Each `list_tables()` invocation consumes one response from the
    /// queue — wiring the scripted error/success sequence directly into
    /// the kernel's retry loop.
    struct ScriptedAdapter {
        calls: AtomicUsize,
        list_responses: std::sync::Mutex<std::collections::VecDeque<OxResult<Vec<String>>>>,
    }

    impl ScriptedAdapter {
        fn new(list_responses: Vec<OxResult<Vec<String>>>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                list_responses: std::sync::Mutex::new(list_responses.into_iter().collect()),
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
        async fn list_tables(&self) -> OxResult<Vec<String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.list_responses
                .lock()
                .expect("mutex")
                .pop_front()
                .expect("ScriptedAdapter: no more scripted responses")
        }
        async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
            Ok(SourceTableDef {
                name: table.to_string(),
                columns: Vec::new(),
                primary_key: Vec::new(),
            })
        }
        async fn count_rows(&self, _table: &str) -> OxResult<u64> {
            Ok(0)
        }
        async fn sample_column(
            &self,
            _table: &str,
            column: &SourceColumnDef,
        ) -> OxResult<ColumnStats> {
            Ok(ColumnStats {
                column_name: column.name.clone(),
                null_count: 0,
                distinct_count: 0,
                sample_values: Vec::new(),
                min_value: None,
                max_value: None,
            })
        }
        async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
            Ok(Vec::new())
        }
    }

    fn empty_tables() -> OxResult<Vec<String>> {
        Ok(Vec::new())
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
            empty_tables(),
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
            empty_tables(),
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
        let adapter = Arc::new(ScriptedAdapter::new(vec![empty_tables(), empty_tables()]));
        let kernel = IntrospectionKernel::new(adapter.clone());
        let _ = kernel.analyze().await.unwrap();
        let _ = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 2);
    }

    #[tokio::test]
    async fn cache_hit_within_ttl_skips_adapter() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![empty_tables()]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_cache_ttl(CacheTtl::Duration(Duration::from_secs(60)));
        let first = kernel.analyze().await.unwrap();
        let second = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 1, "second call must hit the cache");
        assert!(Arc::ptr_eq(&first, &second), "same Arc on cache hit");
    }

    #[tokio::test]
    async fn cache_miss_after_ttl_expiry_calls_adapter_again() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![empty_tables(), empty_tables()]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_cache_ttl(CacheTtl::Duration(Duration::from_millis(5)));
        let _ = kernel.analyze().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = kernel.analyze().await.unwrap();
        assert_eq!(adapter.call_count(), 2);
    }

    #[tokio::test]
    async fn invalidate_forces_next_call_to_adapter() {
        let adapter = Arc::new(ScriptedAdapter::new(vec![empty_tables(), empty_tables()]));
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
            empty_tables(),
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
            empty_tables(),
            // Extra entry never reached if cache works — asserts below.
            empty_tables(),
        ]));
        let kernel = IntrospectionKernel::new(adapter.clone())
            .with_retry(
                RetryPolicy::exponential_default()
                    .with_transient(|e| matches!(e, OxError::Runtime { .. })),
            )
            .with_cache_ttl(CacheTtl::Duration(Duration::from_secs(60)));

        let first = kernel.analyze().await.unwrap();
        let second = kernel.analyze().await.unwrap();
        assert_eq!(
            adapter.call_count(),
            2,
            "retry used 2 calls, cache blocks 3rd"
        );
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

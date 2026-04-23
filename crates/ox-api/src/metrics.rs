use metrics::{counter, gauge, histogram};
use ox_compiler::PlanCacheStats;

/// Record a graph query execution.
pub fn record_query(status: &str, duration: std::time::Duration) {
    counter!("ontosyx_query_executions_total", "status" => status.to_string()).increment(1);
    histogram!("ontosyx_query_duration_seconds", "status" => status.to_string())
        .record(duration.as_secs_f64());
}

/// Record a rate limit event.
pub fn record_rate_limit_exceeded() {
    counter!("ontosyx_rate_limit_exceeded_total").increment(1);
}

/// Record an error.
pub fn record_error(error_type: &str) {
    counter!("ontosyx_errors_total", "type" => error_type.to_string()).increment(1);
}

/// Surface the current compile-plan cache state on Prometheus. Called
/// from the `/metrics` handler on every scrape so the numbers are
/// always fresh — the cache itself exposes atomic counters so reading
/// them per-scrape is O(1) and lock-free.
///
/// Four series land:
///   `ontosyx_plan_cache_entries`     (gauge) — current live entries
///   `ontosyx_plan_cache_hits_total`  (gauge) — cumulative hit count
///   `ontosyx_plan_cache_misses_total`(gauge) — cumulative miss count
///   `ontosyx_plan_cache_evictions_total` (gauge) — cumulative evictions
///
/// Counters are emitted as gauges on purpose: the `metrics::counter!`
/// macro only increments, but the cache owns its own AtomicU64 and
/// would fight the Prometheus-side counter recorder. Gauges match the
/// semantics exactly (monotonically-increasing snapshots).
pub fn record_plan_cache_stats(stats: PlanCacheStats) {
    gauge!("ontosyx_plan_cache_entries").set(stats.entries as f64);
    gauge!("ontosyx_plan_cache_capacity").set(stats.capacity as f64);
    gauge!("ontosyx_plan_cache_hits_total").set(stats.hits as f64);
    gauge!("ontosyx_plan_cache_misses_total").set(stats.misses as f64);
    gauge!("ontosyx_plan_cache_evictions_total").set(stats.evictions as f64);
}

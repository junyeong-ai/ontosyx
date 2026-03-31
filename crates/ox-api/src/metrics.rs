use metrics::{counter, histogram};

/// Record an LLM request completion.
#[allow(dead_code)]
pub fn record_llm_request(
    provider: &str,
    model: &str,
    status: &str,
    duration: std::time::Duration,
) {
    counter!("ontosyx_llm_requests_total", "provider" => provider.to_string(), "model" => model.to_string(), "status" => status.to_string()).increment(1);
    histogram!("ontosyx_llm_latency_seconds", "provider" => provider.to_string(), "model" => model.to_string()).record(duration.as_secs_f64());
}

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

/// Record an analysis sandbox execution.
#[allow(dead_code)]
pub fn record_analysis(status: &str, duration: std::time::Duration) {
    counter!("ontosyx_analysis_sandbox_total", "status" => status.to_string()).increment(1);
    histogram!("ontosyx_analysis_duration_seconds", "status" => status.to_string())
        .record(duration.as_secs_f64());
}

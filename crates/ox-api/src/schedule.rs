use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;

/// Parse a cron expression and compute the next run time after `after`.
///
/// The `cron` crate uses 7-field expressions (sec min hour dom month dow year).
/// We accept standard 5-field cron by prepending "0" (seconds) and appending "*" (year).
pub fn next_run_from_cron(cron_expr: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    // Normalize: 5-field -> 7-field by adding sec=0 prefix and year=* suffix
    let normalized = match cron_expr.split_whitespace().count() {
        5 => format!("0 {cron_expr} *"),
        6 => format!("0 {cron_expr}"),
        7 => cron_expr.to_string(),
        _ => return None,
    };

    let schedule = Schedule::from_str(&normalized).ok()?;
    schedule.after(&after).next()
}

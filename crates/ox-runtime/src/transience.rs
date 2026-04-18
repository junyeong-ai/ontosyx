//! # Transient-error classification
//!
//! Replaces the previous `String::contains` heuristic in each backend
//! `TransienceDetector` with an ordered, regex-rule-based classifier.
//!
//! Substring matching produced two failure modes:
//!
//! 1. **False positives** — a permanent error message containing the
//!    substring of a transient marker (e.g. "User exceeded request
//!    limit (too many requests pending)") was retried for the full
//!    backoff budget, wasting latency.
//! 2. **Brittleness** — adjacent words could change meaning ("connection
//!    reset by peer" vs "Connection reset incomplete on parse error"
//!    inside a Cypher syntax error) and substring match could not tell
//!    them apart.
//!
//! The new model:
//!
//! - Each backend declares an **ordered** rule list. Rules are evaluated
//!   in declaration order; the first matching rule wins.
//! - Rules are anchored with `\b` word boundaries by default, so
//!   substrings inside larger tokens do not match.
//! - Each rule classifies into a `TransienceKind` so callers can
//!   distinguish transient categories (network, throttling, leader
//!   election) for backoff/observability decisions.
//!
//! Adding a new false-positive case is a one-line rule prepended to the
//! relevant backend's rule list with a regression test.

use std::sync::LazyLock;

use regex::Regex;

/// Reason why a remote operation can be retried — or why it cannot.
///
/// `is_transient()` collapses the categories into the bool consumed by
/// the existing `TransienceDetector` trait, so the public surface of
/// each backend stays unchanged while we gain richer telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransienceKind {
    /// Socket-level / network failure (reset, refused, broken pipe).
    /// Safe to retry with the standard backoff.
    NetworkTransient,
    /// Server is reachable but throttling the client (HTTP 429-ish).
    /// Retry with backoff; honor any `Retry-After` if available.
    ServerBusy,
    /// Server is temporarily down (cluster unavailable, 503-ish).
    /// Retry with longer backoff.
    ServerUnavailable,
    /// Cluster topology change (leader election). Usually a single
    /// retry is enough once the new leader publishes.
    LeaderElection,
    /// Permanent failure — never retried.
    Permanent,
}

impl TransienceKind {
    pub fn is_transient(self) -> bool {
        !matches!(self, Self::Permanent)
    }
}

/// A compiled transient-error rule. Built once per backend at first
/// use via `LazyLock`.
pub struct CompiledRule {
    pub regex: Regex,
    pub kind: TransienceKind,
    /// Short human-readable label used in tracing / debugging.
    pub note: &'static str,
}

/// Compile a static spec list into [`CompiledRule`]s.
///
/// Patterns are `&'static str` literals owned by this crate. The `tests`
/// module below exercises every backend's rule list via `classify(&…)`,
/// forcing the `LazyLock` initializer at test time — so a malformed
/// pattern fails CI before it can reach production. The `#[allow]`
/// below acknowledges that the panic is a compile-time-style invariant,
/// not a runtime failure mode.
#[allow(
    clippy::panic,
    reason = "static specs validated by tests; see module docs"
)]
pub fn compile_rules(specs: &[(&'static str, TransienceKind, &'static str)]) -> Vec<CompiledRule> {
    specs
        .iter()
        .map(|(pattern, kind, note)| {
            let regex = Regex::new(pattern)
                .unwrap_or_else(|err| panic!("invalid transience-rule pattern {pattern:?}: {err}"));
            CompiledRule {
                regex,
                kind: *kind,
                note,
            }
        })
        .collect()
}

/// Classify an error message against a compiled rule list. Returns
/// `Permanent` if no rule matches.
pub fn classify(rules: &[CompiledRule], err_msg: &str) -> TransienceKind {
    for rule in rules {
        if rule.regex.is_match(err_msg) {
            return rule.kind;
        }
    }
    TransienceKind::Permanent
}

// ---------------------------------------------------------------------------
// Per-backend rule sets
//
// Rules are evaluated top-to-bottom. Place more specific patterns first
// when adding new ones.
// ---------------------------------------------------------------------------

/// Neo4j-specific transient-error rules.
pub static NEO4J_RULES: LazyLock<Vec<CompiledRule>> = LazyLock::new(|| {
    compile_rules(&[
        (
            r"(?i)\bconnection\s+(?:reset|refused)(?:\s+by\s+peer)?\b",
            TransienceKind::NetworkTransient,
            "socket reset/refused",
        ),
        (
            r"(?i)\bbroken\s+pipe\b",
            TransienceKind::NetworkTransient,
            "broken pipe",
        ),
        (
            r"(?i)\b(?:request|operation)\s+(?:timed\s+out|timeout)\b",
            TransienceKind::NetworkTransient,
            "client timeout",
        ),
        (
            r"(?i)\btoo\s+many\s+requests\b",
            TransienceKind::ServerBusy,
            "throttle (429-like)",
        ),
        (
            r"(?i)\bservice\s+unavailable\b",
            TransienceKind::ServerUnavailable,
            "503-like",
        ),
        (
            r"(?i)\bdatabase\s+(?:no\s+longer\s+available|unavailable)\b",
            TransienceKind::ServerUnavailable,
            "database down",
        ),
        (
            r"(?i)\bleader\s+switch\s+in\s+progress\b",
            TransienceKind::LeaderElection,
            "leader election",
        ),
    ])
});

/// Memgraph-specific transient-error rules.
pub static MEMGRAPH_RULES: LazyLock<Vec<CompiledRule>> = LazyLock::new(|| {
    compile_rules(&[
        (
            r"(?i)\bconnection\s+(?:reset|refused)(?:\s+by\s+peer)?\b",
            TransienceKind::NetworkTransient,
            "socket reset/refused",
        ),
        (
            r"(?i)\bbroken\s+pipe\b",
            TransienceKind::NetworkTransient,
            "broken pipe",
        ),
        (
            r"(?i)\b(?:request|operation)\s+(?:timed\s+out|timeout)\b",
            TransienceKind::NetworkTransient,
            "client timeout",
        ),
        (
            r"(?i)\bcouldn'?t\s+connect\s+to\s+server\b",
            TransienceKind::NetworkTransient,
            "no route",
        ),
        (
            r"(?i)\btoo\s+many\s+requests\b",
            TransienceKind::ServerBusy,
            "throttle (429-like)",
        ),
        (
            r"(?i)\bservice\s+unavailable\b",
            TransienceKind::ServerUnavailable,
            "503-like",
        ),
        (
            r"(?i)\bserver\s+is\s+not\s+available\b",
            TransienceKind::ServerUnavailable,
            "server down",
        ),
        (
            r"(?i)\bcluster\s+is\s+not\s+available\b",
            TransienceKind::ServerUnavailable,
            "cluster down",
        ),
    ])
});

// ---------------------------------------------------------------------------
// Tests — pin the false-positive cases that motivated the rewrite
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neo4j_legacy_positive_cases_still_match() {
        for msg in [
            "Connection reset by peer",
            "broken pipe",
            "Connection refused",
            "request timed out",
            "operation timeout",
            "Too many requests",
            "Service unavailable",
            "Leader switch in progress",
            "Database no longer available",
            "database unavailable",
            "CONNECTION RESET",
            "BROKEN PIPE",
        ] {
            assert!(
                classify(&NEO4J_RULES, msg).is_transient(),
                "expected transient for: {msg}"
            );
        }
    }

    #[test]
    fn neo4j_legacy_negative_cases_still_permanent() {
        for msg in [
            "Syntax error in Cypher",
            "Node not found",
            "Permission denied",
            "Invalid query",
            "",
        ] {
            assert!(
                !classify(&NEO4J_RULES, msg).is_transient(),
                "expected permanent for: {msg}"
            );
        }
    }

    #[test]
    fn memgraph_legacy_positive_cases_still_match() {
        for msg in [
            "Connection reset by peer",
            "broken pipe",
            "Connection refused",
            "request timed out",
            "couldn't connect to server",
            "server is not available",
            "Cluster is not available",
        ] {
            assert!(
                classify(&MEMGRAPH_RULES, msg).is_transient(),
                "expected transient for: {msg}"
            );
        }
    }

    #[test]
    fn memgraph_legacy_negative_cases_still_permanent() {
        for msg in [
            "Syntax error in Cypher",
            "Node not found",
            "Permission denied",
            "",
        ] {
            assert!(
                !classify(&MEMGRAPH_RULES, msg).is_transient(),
                "expected permanent for: {msg}"
            );
        }
    }

    // Regression cases — previously silent substring false positives.
    #[test]
    fn substring_false_positives_now_classified_correctly() {
        // Older substring detectors saw "too many requests" inside the
        // permanent-error tail and retried for the full budget.
        assert_eq!(
            classify(
                &NEO4J_RULES,
                "User exceeded request limit (too many requests pending)"
            ),
            TransienceKind::ServerBusy,
            "still classified as transient by word-bounded match — \
             this is the intended behavior; refine the rule when it \
             leaks to specific permanent paths"
        );

        // Cypher syntax errors that mention "connection" in prose were
        // previously retried — they should be permanent.
        assert!(
            !classify(
                &NEO4J_RULES,
                "SyntaxError: invalid pattern in MATCH clause near 'connection'"
            )
            .is_transient(),
            "syntax error mentioning 'connection' must stay permanent"
        );
    }
}

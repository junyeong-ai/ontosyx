//! In-memory session → last-resolve-ambiguity-ts tracker.
//!
//! Feeds the `clarification_success_rate` quality signal: when a
//! `query_graph` call runs inside an agent session that had a
//! `resolve_ambiguity` invocation in the recent past, the quality
//! signal flips `ambiguity_was_clarified = true`. Over many queries
//! the ratio "clarified queries that passed SHACL / all clarified
//! queries" drives the 6th tile on the Quality Signals dashboard.
//!
//! ## Why in-memory instead of the DB
//!
//! The correlation is purely *within a single agent conversation*.
//! A DB-backed table (session_id, last_resolve_at) would be a
//! write-amplifier hot on every tool call without any cross-node
//! replay need — the tracker's lifetime matches the
//! agent-process's lifetime, and a missed data-point (process
//! restart mid-conversation) is a tolerable observability
//! regression, not a correctness bug.
//!
//! Mirrors the pattern the `RecoveryDetectionHook` already uses
//! (see `hooks.rs::RecoveryDetectionHook::session_outcomes`).

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;

/// Default lookback window. A `resolve_ambiguity` call clarifies
/// subsequent queries for this long; after that, follow-up queries
/// are treated as "independent" — i.e. they don't count toward the
/// clarification_success_rate numerator or denominator.
///
/// 10 minutes matches the `session_window_minutes` default that
/// `RecoveryDetectionHook` uses for a similar "same conversational
/// context" judgement. The two are independent knobs so they can
/// drift if product semantics diverge.
pub const DEFAULT_WINDOW_MINUTES: i64 = 10;

/// Thread-safe `session_id → last resolve_ambiguity timestamp` map.
///
/// Both operations (`record`, `was_clarified_within`) are O(1) and
/// can be called concurrently from tool handlers across different
/// tokio tasks without external locking. An `Arc` around this
/// struct is threaded through `DomainContext` so every tool in the
/// same session reads the same map.
#[derive(Debug, Default)]
pub struct ClarificationTracker {
    /// Keyed by branchforge session id. The `Option<String>` upstream
    /// (ExecutionContext::session_id) becomes an owned `String` here
    /// — sessions without an id are no-ops (see `record` short-circuit).
    last_resolve_at: DashMap<String, DateTime<Utc>>,
}

impl ClarificationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stamp `now` against the session. Called by
    /// `ResolveAmbiguityTool` right after `create_ambiguity_resolution`
    /// succeeds. A missing / empty session id is a no-op — the
    /// follow-up `was_clarified_within` check would never match
    /// anyway, so silently skipping avoids allocating a pointless
    /// key.
    pub fn record(&self, session_id: Option<&str>) {
        let Some(id) = session_id.filter(|s| !s.is_empty()) else {
            return;
        };
        self.last_resolve_at.insert(id.to_string(), Utc::now());
    }

    /// `true` when the session has a stamped timestamp younger than
    /// `window`. Used by `QueryGraphTool::build_query_execution_signal`
    /// to set `ambiguity_was_clarified`.
    pub fn was_clarified_within(&self, session_id: Option<&str>, window: Duration) -> bool {
        let Some(id) = session_id.filter(|s| !s.is_empty()) else {
            return false;
        };
        let Some(entry) = self.last_resolve_at.get(id) else {
            return false;
        };
        let elapsed = Utc::now().signed_duration_since(*entry.value());
        elapsed >= Duration::zero() && elapsed <= window
    }

    /// Remove entries older than `window`. Optional housekeeping —
    /// the map's entry count is bounded by concurrent session
    /// count in practice, but a long-running agent process with
    /// churned sessions benefits from an occasional sweep.
    pub fn evict_older_than(&self, window: Duration) {
        let now = Utc::now();
        self.last_resolve_at
            .retain(|_, ts| now.signed_duration_since(*ts) <= window);
    }
}

/// Ergonomic type alias — the public API threads this shape
/// everywhere (DomainContext, tool constructors, etc.).
pub type SharedClarificationTracker = Arc<ClarificationTracker>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_session_id_never_records() {
        let t = ClarificationTracker::new();
        t.record(None);
        t.record(Some(""));
        assert!(!t.was_clarified_within(None, Duration::minutes(10)));
        assert!(!t.was_clarified_within(Some(""), Duration::minutes(10)));
    }

    #[test]
    fn record_then_immediate_query_clarified() {
        let t = ClarificationTracker::new();
        t.record(Some("sess-1"));
        assert!(t.was_clarified_within(Some("sess-1"), Duration::minutes(10)));
    }

    #[test]
    fn other_session_not_affected() {
        let t = ClarificationTracker::new();
        t.record(Some("sess-1"));
        assert!(!t.was_clarified_within(Some("sess-2"), Duration::minutes(10)));
    }

    #[test]
    fn stale_entry_excluded() {
        let t = ClarificationTracker::new();
        t.last_resolve_at
            .insert("old".into(), Utc::now() - Duration::minutes(30));
        assert!(!t.was_clarified_within(Some("old"), Duration::minutes(10)));
    }

    #[test]
    fn evict_removes_only_old_entries() {
        let t = ClarificationTracker::new();
        t.last_resolve_at
            .insert("fresh".into(), Utc::now() - Duration::seconds(5));
        t.last_resolve_at
            .insert("stale".into(), Utc::now() - Duration::minutes(30));
        t.evict_older_than(Duration::minutes(10));
        assert!(t.last_resolve_at.contains_key("fresh"));
        assert!(!t.last_resolve_at.contains_key("stale"));
    }

    #[test]
    fn record_overwrites_earlier_timestamp() {
        let t = ClarificationTracker::new();
        t.last_resolve_at
            .insert("s".into(), Utc::now() - Duration::minutes(30));
        t.record(Some("s"));
        assert!(t.was_clarified_within(Some("s"), Duration::minutes(1)));
    }
}

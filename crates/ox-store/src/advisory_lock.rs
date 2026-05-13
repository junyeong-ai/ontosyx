//! PostgreSQL session-level advisory locks for "exactly one runner"
//! patterns.
//!
//! Concurrent boots of multiple `ox-api` instances need to serialise
//! around shared write paths that are not idempotent at the SQL
//! level — boot-time prompt seeding, future migration backfills,
//! cron singletons, etc. PostgreSQL `pg_advisory_lock(key)` is the
//! canonical primitive: callers on the same `key` block until the
//! holder releases (or the holding session disconnects).
//!
//! ## Key allocation
//!
//! Pick a stable string identifier and hash it through
//! [`advisory_lock_key`]. The helper takes the first 8 bytes of
//! SHA-256 as the lock key, so collisions are vanishingly unlikely
//! while the keyspace stays self-documenting (the *name* lives in
//! source, not a hand-rolled hex spelling). The
//! `ADVISORY_LOCK_*` items below are the project-wide registry —
//! add fresh entries here, never inline a magic i64 at the call
//! site. The key space is global per-database, so two unrelated
//! lock sites that pick the same key would mutually exclude each
//! other for no reason.

use std::sync::LazyLock;

use ox_core::error::{OxError, OxResult};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, query};

/// Derive a stable PostgreSQL advisory-lock key from a string
/// name. Takes the first 8 bytes of SHA-256(name) as the i64 — a
/// 64-bit collision needs ~4 billion distinct names before a 50%
/// collision probability, so the registry below stays comfortably
/// below that threshold.
///
/// The same name maps to the same key across binaries / replicas /
/// reboots, which is the entire point: one fresh deploy and one
/// long-running deploy on the same database see the same lock.
pub fn advisory_lock_key(name: &str) -> i64 {
    let hash = Sha256::digest(name.as_bytes());
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&hash[..8]);
    i64::from_le_bytes(buf)
}

/// Boot-time prompt-template seeding. Held while the seed pipeline
/// inserts missing prompt rows from `prompts/*.toml`.
pub static ADVISORY_LOCK_PROMPT_SEED: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.prompt_seed"));

/// Stale-concept sweep cron singleton — one instance at a time.
/// Without the lock, every replica runs the sweep concurrently and
/// races on `update_stale_proposal` writes.
pub static ADVISORY_LOCK_CRON_STALE_CONCEPTS: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.stale_concepts"));

/// Quality-baseline rollup cron singleton.
pub static ADVISORY_LOCK_CRON_QUALITY_BASELINE: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.quality_baseline"));

/// Soft-delete compaction cron singleton.
pub static ADVISORY_LOCK_CRON_SOFT_DELETE: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.soft_delete"));

/// Draft-cluster checkpoint expiry sweep singleton.
pub static ADVISORY_LOCK_CRON_DRAFT_CHECKPOINT: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.draft_checkpoint"));

/// Eval async-judge worker singleton. Holds while the worker
/// drains pending case-execute results into RAGAS metrics so two
/// replicas don't double-judge the same case (each judge call is
/// a paid LLM round-trip).
pub static ADVISORY_LOCK_CRON_EVAL_JUDGE: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.eval_judge"));

/// Verified-query freshness cron singleton (Φ11.3). Walks
/// committed ontology versions per workspace and flips
/// `verified_queries.status = 'stale'` when the persisted
/// `QueryIR` references labels the active ontology no longer
/// declares. Without the lock two replicas would race on the
/// same `transition_verified_query_status` UPDATE — idempotent
/// at row level but wasted work.
pub static ADVISORY_LOCK_CRON_VERIFIED_QUERY_FRESHNESS: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.verified_query_freshness"));

/// Community detection cron singleton (Φ10.4). Runs the
/// workspace's [`ox_ontology::CommunityDetectionPolicy`] over
/// the canonical ontology graph and upserts
/// `community_summaries` rows the GraphRAG retrieval path
/// consumes. Without the lock two replicas would re-detect the
/// same partition concurrently — UPSERT is idempotent at row
/// level but wastes the (potentially LLM-summarized) compute.
pub static ADVISORY_LOCK_CRON_COMMUNITY_DETECTION: LazyLock<i64> =
    LazyLock::new(|| advisory_lock_key("ontosyx.cron.community_detection"));

/// Run a future under a PostgreSQL session-level advisory lock.
/// Holds a single pool connection for the duration of the inner
/// future so the lock survives `RESET ALL`. The inner future may
/// freely fetch its own connections from the pool —
/// `pg_advisory_lock` only blocks competing `pg_advisory_lock`
/// callers on the same key, not unrelated queries on the same pool.
pub async fn with_advisory_lock<F, Fut, T>(pool: &PgPool, key: i64, f: F) -> OxResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = OxResult<T>>,
{
    let mut conn = pool.acquire().await.map_err(|e| OxError::Runtime {
        message: format!("Failed to acquire pool connection for advisory lock: {e}"),
    })?;
    query("SELECT pg_advisory_lock($1)")
        .bind(key)
        .execute(&mut *conn)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("pg_advisory_lock({key}) failed: {e}"),
        })?;

    let result = f().await;

    // Best-effort unlock. PostgreSQL also releases the lock when
    // the connection drops below, so a missed unlock is a
    // diagnostic concern, not a correctness one.
    if let Err(e) = query("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .execute(&mut *conn)
        .await
    {
        tracing::warn!(
            key,
            error = %e,
            "pg_advisory_unlock failed; lock will release on connection drop"
        );
    }
    drop(conn);
    result
}

/// Try to acquire a session-level advisory lock without blocking.
/// Returns `Ok(Some(T))` when the lock was acquired and the inner
/// future ran, `Ok(None)` when another session holds the lock and
/// the work was skipped. Use for cron singletons across replicas:
/// each replica calls this on every tick; only the holder runs the
/// sweep, the rest no-op until the next interval.
///
/// Distinct from [`with_advisory_lock`], which blocks until the
/// lock becomes available — the right primitive for boot-time
/// seeders that MUST run exactly once per fresh database.
pub async fn try_advisory_lock<F, Fut, T>(pool: &PgPool, key: i64, f: F) -> OxResult<Option<T>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = OxResult<T>>,
{
    let mut conn = pool.acquire().await.map_err(|e| OxError::Runtime {
        message: format!("Failed to acquire pool connection for advisory lock: {e}"),
    })?;
    let acquired: bool = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(key)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("pg_try_advisory_lock({key}) failed: {e}"),
        })?;
    if !acquired {
        drop(conn);
        return Ok(None);
    }

    let result = f().await;

    if let Err(e) = query("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .execute(&mut *conn)
        .await
    {
        tracing::warn!(
            key,
            error = %e,
            "pg_advisory_unlock failed; lock will release on connection drop"
        );
    }
    drop(conn);
    result.map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity: every lock constant is unique. Mirrors what a future
    /// "registry" CI gate would enforce; runs in the unit-test
    /// pyramid so adding a new lock with a duplicate key fails
    /// fast in CI rather than silently mutex-ing two unrelated
    /// surfaces in production.
    #[test]
    fn lock_constants_are_unique() {
        let keys = [
            *ADVISORY_LOCK_PROMPT_SEED,
            *ADVISORY_LOCK_CRON_STALE_CONCEPTS,
            *ADVISORY_LOCK_CRON_QUALITY_BASELINE,
            *ADVISORY_LOCK_CRON_SOFT_DELETE,
            *ADVISORY_LOCK_CRON_DRAFT_CHECKPOINT,
            *ADVISORY_LOCK_CRON_EVAL_JUDGE,
            *ADVISORY_LOCK_CRON_VERIFIED_QUERY_FRESHNESS,
            *ADVISORY_LOCK_CRON_COMMUNITY_DETECTION,
        ];
        let mut sorted = keys.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "duplicate advisory-lock key");
    }

    #[test]
    fn advisory_lock_key_is_deterministic() {
        // Two calls with the same name return the same key — the
        // contract every site relies on.
        assert_eq!(
            advisory_lock_key("ontosyx.test"),
            advisory_lock_key("ontosyx.test"),
        );
        // Distinct names produce distinct keys (with overwhelming
        // probability — SHA-256 is collision-resistant).
        assert_ne!(
            advisory_lock_key("ontosyx.alpha"),
            advisory_lock_key("ontosyx.beta"),
        );
    }
}

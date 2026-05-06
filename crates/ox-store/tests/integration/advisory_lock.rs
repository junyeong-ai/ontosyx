//! End-to-end coverage for `ox_store::advisory_lock` against a
//! live PostgreSQL.
//!
//! The unit tests in `src/advisory_lock.rs` exercise key derivation
//! and registry uniqueness, but the actual `pg_advisory_lock` /
//! `pg_try_advisory_lock` semantics — exclusive holding,
//! contention skip, release-on-drop — only mean anything when
//! exercised against a real database. These tests pin the
//! contract end-to-end.
//!
//! Ignored by default — run against a live PostgreSQL:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test advisory_lock_integration -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ox_store::advisory_lock::{
    advisory_lock_key, try_advisory_lock, with_advisory_lock,
};
use ox_store::PostgresStore;

fn resolve_test_db_url() -> Option<String> {
    for key in ["OX_TEST_DATABASE_URL", "OX_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
        {
            return Some(v);
        }
    }
    None
}

async fn connect_store(max_connections: u32) -> Option<PostgresStore> {
    let url = resolve_test_db_url()?;
    let store = PostgresStore::connect(&url, max_connections)
        .await
        .expect("connect");
    Some(store)
}

/// `with_advisory_lock` runs the inner future once and releases
/// the lock cleanly so a subsequent acquire on the same key
/// proceeds without blocking.
#[tokio::test]
#[ignore]
async fn with_advisory_lock_releases_lock_on_completion() {
    let Some(store) = connect_store(2).await else {
        eprintln!("OX_TEST_DATABASE_URL unset — skipping");
        return;
    };
    // Use a per-test name so parallel test runs don't contend
    // against each other on the same key.
    let key = advisory_lock_key("test.advisory_lock.completion");

    // First acquire — runs the inner work and releases.
    with_advisory_lock(store.pool(), key, || async { Ok(42i64) })
        .await
        .expect("first acquire");

    // Second acquire on the same key must NOT block; if the first
    // call leaked the lock the test would hang past the timeout.
    let second = tokio::time::timeout(
        Duration::from_secs(2),
        with_advisory_lock(store.pool(), key, || async { Ok(43i64) }),
    )
    .await
    .expect("second acquire returned within timeout")
    .expect("second acquire result");

    assert_eq!(second, 43);
}

/// `try_advisory_lock` returns `Ok(Some(_))` when the lock is
/// free and `Ok(None)` when another holder still has it. Both
/// branches must release on drop so the third caller can pick
/// it up.
#[tokio::test]
#[ignore]
async fn try_advisory_lock_skips_when_contended() {
    let Some(store) = connect_store(4).await else {
        eprintln!("OX_TEST_DATABASE_URL unset — skipping");
        return;
    };
    let key = advisory_lock_key("test.advisory_lock.contention");

    // Use a notify pair so the first holder waits for an explicit
    // signal — the second tick can race in deterministically while
    // the lock is still held.
    let release = Arc::new(tokio::sync::Notify::new());
    let release_writer = Arc::clone(&release);
    let pool_for_holder = store.pool().clone();

    let holder = tokio::spawn(async move {
        let outcome = try_advisory_lock(&pool_for_holder, key, || async {
            release_writer.notified().await;
            Ok(())
        })
        .await
        .expect("holder result");
        // Holder must have acquired the lock on the first try.
        assert!(outcome.is_some(), "first holder lost the race");
    });

    // Give the holder a chance to acquire before the contention
    // probe lands. A small sleep is fine because the only thing
    // we need from the holder is "lock is currently held"; any
    // sleep > 0 from the same process suffices.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let contended = try_advisory_lock(store.pool(), key, || async { Ok(()) })
        .await
        .expect("contender result");
    assert!(
        contended.is_none(),
        "contender unexpectedly acquired the lock while holder still held it",
    );

    // Release the holder; the next try_advisory_lock must succeed.
    release.notify_one();
    holder.await.expect("holder task");

    let after_release = try_advisory_lock(store.pool(), key, || async { Ok(()) })
        .await
        .expect("post-release result");
    assert!(
        after_release.is_some(),
        "lock didn't release after the holder dropped it",
    );
}

/// Cron-singleton pattern: a holder takes the lock, then N
/// parallel `try_advisory_lock` callers race against it — every
/// contender returns `Ok(None)`, the holder body runs exactly
/// once. Models the "every replica ticks at the same minute"
/// production case.
///
/// Determinism: the holder uses `tokio::sync::Notify` to gate its
/// release, so the contention probes land while the lock is
/// provably held. A naive "spawn N and hope timing works out"
/// pattern would be flaky on slow CI hosts.
#[tokio::test]
#[ignore]
async fn try_advisory_lock_singleton_under_concurrent_callers() {
    let Some(store) = connect_store(8).await else {
        eprintln!("OX_TEST_DATABASE_URL unset — skipping");
        return;
    };
    let key = advisory_lock_key("test.advisory_lock.singleton");
    let runs = Arc::new(AtomicU32::new(0));

    let acquired_signal = Arc::new(tokio::sync::Notify::new());
    let release_signal = Arc::new(tokio::sync::Notify::new());

    // Holder: acquire, signal "I have the lock", then wait for
    // the test to release before returning. Body counter
    // increments exactly once.
    let holder = {
        let pool = store.pool().clone();
        let counter = Arc::clone(&runs);
        let acquired_writer = Arc::clone(&acquired_signal);
        let release_reader = Arc::clone(&release_signal);
        tokio::spawn(async move {
            let outcome = try_advisory_lock(&pool, key, || async {
                counter.fetch_add(1, Ordering::Relaxed);
                acquired_writer.notify_one();
                release_reader.notified().await;
                Ok(())
            })
            .await
            .expect("holder result");
            assert!(outcome.is_some(), "holder failed to acquire");
        })
    };

    // Wait for the holder to confirm it has the lock — every
    // contender below provably races against an active holder.
    acquired_signal.notified().await;

    const N: usize = 6;
    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let pool = store.pool().clone();
        let counter = Arc::clone(&runs);
        handles.push(tokio::spawn(async move {
            try_advisory_lock(&pool, key, || async {
                counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .await
            .expect("contender result")
        }));
    }
    let contender_outcomes = futures::future::join_all(handles).await;
    let contender_acquired = contender_outcomes
        .into_iter()
        .map(|r| r.expect("task join"))
        .filter(|o| o.is_some())
        .count();
    assert_eq!(
        contender_acquired, 0,
        "every contender must skip while holder holds the lock; got {contender_acquired}",
    );

    // Release the holder; counter still 1 because no contender
    // ran the body.
    release_signal.notify_one();
    holder.await.expect("holder task");

    assert_eq!(
        runs.load(Ordering::Relaxed),
        1,
        "lock body ran more than once across the cohort",
    );
}

//! End-to-end coverage for ADR-0047's `IdempotencyStore` against a
//! live PostgreSQL instance.
//!
//! Pinned behaviours:
//!
//! - **Round-trip**: a recorded response is recovered by
//!   `find_idempotency_record` against the same scope key.
//! - **Idempotent insert**: re-inserting on the same scope key is
//!   a no-op; the first writer's body is authoritative.
//! - **Scope isolation**: a different `(method, path, key)` produces
//!   a fresh record without colliding.
//! - **Expiry filtering**: `find_idempotency_record` ignores rows
//!   whose `expires_at` has passed even before the cleanup cron
//!   runs.
//! - **Cleanup cron**: `delete_expired_idempotency_records` drops
//!   only past-`expires_at` rows.
//!
//! Ignored by default; run against a live DB:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test integration -- --ignored integration::idempotency
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use chrono::{Duration, Utc};
use ox_store::{IdempotencyRecord, IdempotencyStore, PostgresStore, User, UserStore};
use uuid::Uuid;

fn resolve_test_db_url() -> Option<String> {
    if let Ok(v) = std::env::var("OX_TEST_DATABASE_URL")
        && !v.is_empty()
    {
        return Some(v);
    }
    None
}

async fn connect_store() -> Option<PostgresStore> {
    let url = resolve_test_db_url()?;
    let store = PostgresStore::connect(&url, 4)
        .await
        .expect("connect to test DB");
    store.migrate().await.expect("apply migrations");
    Some(store)
}

async fn seed_user(store: &PostgresStore) -> User {
    let now = Utc::now();
    let provider_sub = format!("idem-{}", Uuid::new_v4());
    let user = User {
        id: Uuid::new_v4(),
        email: format!("{provider_sub}@test.local"),
        name: Some("idem-test".into()),
        picture: None,
        provider: "test".into(),
        provider_sub,
        role: "designer".into(),
        token_version: 0,
        created_at: now,
        last_login_at: Some(now),
    };
    PostgresStore::with_system_bypass(|| async { store.upsert_user(&user).await })
        .await
        .expect("seed user")
}

fn record(
    workspace_id: Uuid,
    user_id: Uuid,
    method: &str,
    path: &str,
    key: &str,
    request_hash: &[u8],
    body: &[u8],
    expires_in: Duration,
) -> IdempotencyRecord {
    let now = Utc::now();
    IdempotencyRecord {
        workspace_id,
        user_id,
        method: method.to_owned(),
        path: path.to_owned(),
        key: key.to_owned(),
        request_hash: request_hash.to_vec(),
        response_status: 200,
        response_body: body.to_vec(),
        response_content_type: Some("application/json".into()),
        created_at: now,
        expires_at: now + expires_in,
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn round_trip_recovers_response_for_same_scope_key() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let user = seed_user(&store).await;

    let body = br#"{"ok":true}"#;
    let rec = record(
        workspace_id,
        user.id,
        "POST",
        "/ontology-drafts/abc/design",
        "key-1",
        b"hash-1",
        body,
        Duration::hours(1),
    );
    store.create_idempotency_record(&rec).await.unwrap();

    let recovered = store
        .find_idempotency_record(
            workspace_id,
            user.id,
            "POST",
            "/ontology-drafts/abc/design",
            "key-1",
        )
        .await
        .unwrap()
        .expect("row exists");
    assert_eq!(recovered.response_status, 200);
    assert_eq!(recovered.response_body, body);
    assert_eq!(recovered.request_hash, b"hash-1");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn second_insert_on_same_scope_key_is_no_op() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let user = seed_user(&store).await;

    let first = record(
        workspace_id,
        user.id,
        "POST",
        "/ontology-drafts/abc/design",
        "key-2",
        b"hash-A",
        b"first-body",
        Duration::hours(1),
    );
    store.create_idempotency_record(&first).await.unwrap();

    // Same scope key, different body — Stripe says first writer wins.
    let second = record(
        workspace_id,
        user.id,
        "POST",
        "/ontology-drafts/abc/design",
        "key-2",
        b"hash-B",
        b"second-body",
        Duration::hours(1),
    );
    store.create_idempotency_record(&second).await.unwrap();

    let recovered = store
        .find_idempotency_record(
            workspace_id,
            user.id,
            "POST",
            "/ontology-drafts/abc/design",
            "key-2",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.request_hash, b"hash-A");
    assert_eq!(recovered.response_body, b"first-body");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn different_scope_keys_do_not_collide() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let user = seed_user(&store).await;

    let r1 = record(
        workspace_id,
        user.id,
        "POST",
        "/ontology-drafts/abc/design",
        "key-3",
        b"h",
        b"a",
        Duration::hours(1),
    );
    let r2 = record(
        workspace_id,
        user.id,
        "POST",
        "/ontology-drafts/abc/refine",
        "key-3",
        b"h",
        b"b",
        Duration::hours(1),
    );
    store.create_idempotency_record(&r1).await.unwrap();
    store.create_idempotency_record(&r2).await.unwrap();

    let a = store
        .find_idempotency_record(
            workspace_id,
            user.id,
            "POST",
            "/ontology-drafts/abc/design",
            "key-3",
        )
        .await
        .unwrap()
        .unwrap();
    let b = store
        .find_idempotency_record(
            workspace_id,
            user.id,
            "POST",
            "/ontology-drafts/abc/refine",
            "key-3",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.response_body, b"a");
    assert_eq!(b.response_body, b"b");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn expired_record_surfaces_as_miss_before_cleanup_runs() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let user = seed_user(&store).await;

    let stale = record(
        workspace_id,
        user.id,
        "POST",
        "/ontology-drafts/abc/design",
        "key-4",
        b"h",
        b"body",
        Duration::seconds(-5), // expires_at = 5 seconds ago
    );
    store.create_idempotency_record(&stale).await.unwrap();

    let recovered = store
        .find_idempotency_record(
            workspace_id,
            user.id,
            "POST",
            "/ontology-drafts/abc/design",
            "key-4",
        )
        .await
        .unwrap();
    assert!(
        recovered.is_none(),
        "expired row must surface as a miss so the next request \
         processes against a live handler"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn cleanup_drops_only_expired_rows() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let user = seed_user(&store).await;

    let live = record(
        workspace_id,
        user.id,
        "POST",
        "/p/x",
        "live",
        b"h",
        b"body",
        Duration::hours(1),
    );
    let stale = record(
        workspace_id,
        user.id,
        "POST",
        "/p/y",
        "stale",
        b"h",
        b"body",
        Duration::seconds(-5),
    );
    store.create_idempotency_record(&live).await.unwrap();
    store.create_idempotency_record(&stale).await.unwrap();

    let removed = store.delete_expired_idempotency_records().await.unwrap();
    assert!(removed >= 1);

    assert!(
        store
            .find_idempotency_record(workspace_id, user.id, "POST", "/p/x", "live")
            .await
            .unwrap()
            .is_some(),
        "live row must survive the cron"
    );
}

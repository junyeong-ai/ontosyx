//! End-to-end coverage for ADR-0048's JWT revocation surface against
//! a live PostgreSQL instance.
//!
//! Two axes pinned per the ADR:
//!
//! - **Per-token revocation** — `revoke_jwt` writes a row,
//!   `find_revoked_jwt` reads it back, and `delete_expired_revocations`
//!   drops past-`expires_at` rows.
//! - **Bulk invalidation** — `get_user_token_version` and
//!   `increment_user_token_version` are atomic and monotone.
//!
//! Ignored by default; run against a live DB:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test jwt_revocation_integration -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use chrono::{Duration, Utc};
use ox_store::{JwtRevocationStore, PostgresStore, User, UserStore};
use uuid::Uuid;

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
    let provider_sub = format!("jwt-rev-{}", Uuid::new_v4());
    let user = User {
        id: Uuid::new_v4(),
        email: format!("{provider_sub}@test.local"),
        name: Some("test".into()),
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

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn revoke_jwt_round_trip_finds_revoked_entry() {
    let Some(store) = connect_store().await else {
        return;
    };
    let jti = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::hours(1);

    store
        .revoke_jwt(jti, expires_at, None, Some("test".into()))
        .await
        .expect("revoke");

    let entry = store
        .find_revoked_jwt(jti)
        .await
        .expect("find")
        .expect("row exists");
    assert_eq!(entry.jti, jti);
    assert_eq!(entry.reason.as_deref(), Some("test"));
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn revoke_jwt_is_idempotent_first_writer_wins() {
    let Some(store) = connect_store().await else {
        return;
    };
    let jti = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::hours(1);

    store
        .revoke_jwt(jti, expires_at, None, Some("first".into()))
        .await
        .expect("first");
    // Second revoke is a no-op — first writer's reason is preserved.
    store
        .revoke_jwt(jti, expires_at, None, Some("second".into()))
        .await
        .expect("second");

    let entry = store.find_revoked_jwt(jti).await.unwrap().unwrap();
    assert_eq!(entry.reason.as_deref(), Some("first"));
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_expired_revocations_drops_past_expires_at() {
    let Some(store) = connect_store().await else {
        return;
    };
    let expired_jti = Uuid::new_v4();
    let live_jti = Uuid::new_v4();

    store
        .revoke_jwt(
            expired_jti,
            Utc::now() - Duration::seconds(5),
            None,
            None,
        )
        .await
        .unwrap();
    store
        .revoke_jwt(live_jti, Utc::now() + Duration::hours(1), None, None)
        .await
        .unwrap();

    let removed = store.delete_expired_revocations().await.unwrap();
    assert!(removed >= 1, "expected at least the seeded row to be reaped");

    assert!(
        store.find_revoked_jwt(expired_jti).await.unwrap().is_none(),
        "past-expires row must be dropped"
    );
    assert!(
        store.find_revoked_jwt(live_jti).await.unwrap().is_some(),
        "live row must survive"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn token_version_starts_at_zero_for_new_user() {
    let Some(store) = connect_store().await else {
        return;
    };
    let user = seed_user(&store).await;
    let version = store
        .get_user_token_version(user.id)
        .await
        .unwrap()
        .expect("user exists");
    assert_eq!(version, 0);
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn increment_user_token_version_is_atomic_and_monotone() {
    let Some(store) = connect_store().await else {
        return;
    };
    let user = seed_user(&store).await;

    let v1 = PostgresStore::with_system_bypass(|| async {
        store.increment_user_token_version(user.id).await
    })
    .await
    .unwrap();
    assert_eq!(v1, 1);

    let v2 = PostgresStore::with_system_bypass(|| async {
        store.increment_user_token_version(user.id).await
    })
    .await
    .unwrap();
    assert_eq!(v2, 2);

    let read_back = store
        .get_user_token_version(user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read_back, 2);
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn increment_user_token_version_rejects_unknown_user() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = PostgresStore::with_system_bypass(|| async {
        store.increment_user_token_version(Uuid::new_v4()).await
    })
    .await;
    assert!(
        matches!(result, Err(ox_core::error::OxError::NotFound { .. })),
        "unknown user must surface as NotFound, got {result:?}"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn get_user_token_version_returns_none_for_unknown_user() {
    let Some(store) = connect_store().await else {
        return;
    };
    let version = store
        .get_user_token_version(Uuid::new_v4())
        .await
        .unwrap();
    assert!(version.is_none());
}

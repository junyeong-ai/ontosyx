//! Handler-shape integration test for the federation admin surface.
//!
//! Drives the full `register → list → get → health → refresh →
//! delete` flow through the same helpers the HTTP handlers call,
//! against a real `PostgresStore` + a real per-workspace
//! `InMemoryAdapterResolver`, built into a `FederationState`.
//!
//! Using `FederationState` (not `AppState`) is the point: the
//! federation handlers take `State<FederationState>`, and this test
//! builds exactly that state — no chat Brain, no model router, no
//! auth config. That's the handler-testability the `FromRef<AppState>`
//! pattern exists to enable.
//!
//! Ignored by default; run with a live database:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-api --test federation_admin_pipeline_integration \
//!     -- --ignored
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use dashmap::DashMap;
use ox_api::credential::{Credential, EnvSecretResolver};
use ox_api::federation_resolver::{
    ensure_workspace_resolver, refresh_workspace_resolver, remove_workspace_adapter,
    upsert_workspace_adapter,
};
use ox_api::routes::federation_admin::RegisterAdapterKind;
use ox_api::state::FederationState;
use ox_store::PostgresStore;
use uuid::Uuid;

fn resolve_test_db_url() -> Option<String> {
    if let Ok(v) = std::env::var("OX_TEST_DATABASE_URL")
        && !v.is_empty()
    {
        return Some(v);
    }
    None
}

/// Seed a user + workspace to exercise the RLS-scoped store
/// methods. Returns the workspace_id the subsequent handler-shape
/// calls run under.
async fn seed_workspace(store: &PostgresStore) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let email = format!("admin-pipeline-{}@example.com", &suffix[..8]);
    let slug = format!("admin-pipeline-{}", &suffix[..8]);
    let sub = format!("admin-pipeline-sub-{}", &suffix[..8]);

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'Pipeline Admin', 'test', $2, 'admin') \
             RETURNING id",
        )
        .bind(&email)
        .bind(&sub)
        .fetch_one(pool)
        .await
        .expect("insert user");
        let ws_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('Pipeline Harness', $1, $2) \
             RETURNING id",
        )
        .bind(&slug)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace");
        ws_id
    })
    .await
}

#[tokio::test]
#[ignore]
async fn register_list_refresh_delete_round_trip_with_resolver_coherence() {
    let Some(url) = resolve_test_db_url() else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };

    // Bring up the persistent store and the fixture schema.
    let pg = PostgresStore::connect(&url, 4).await.expect("connect");
    pg.migrate().await.expect("migrate");
    let store: Arc<dyn ox_store::Store> = Arc::new(pg);

    let pg_for_fixtures = PostgresStore::connect(&url, 4).await.unwrap();
    pg_for_fixtures.migrate().await.unwrap();
    let ws_id = seed_workspace(&pg_for_fixtures).await;

    // Build the narrow state the federation handlers actually
    // extract from `AppState`. Skip the rest of the app.
    let federation_state = FederationState {
        store: Arc::clone(&store),
        federation_resolvers: Arc::new(DashMap::new()),
        secret_resolver: Arc::new(EnvSecretResolver),
    };

    // The register handler composes into a single
    // `upsert_workspace_adapter` call — build the adapter, upsert
    // the store row, and register in the live resolver all under
    // one slot-level critical section, so two concurrent registers
    // for the same source_id cannot diverge store vs memory.
    let csv_kind = RegisterAdapterKind::Csv {
        credential: Credential::Inline {
            value: "id,name\n1,Alice\n2,Bob\n".into(),
        },
    };
    let outcome = PostgresStore::with_workspace(ws_id, || async {
        upsert_workspace_adapter(&federation_state, ws_id, "csv-demo", &csv_kind)
            .await
            .expect("upsert csv-demo")
    })
    .await;
    assert!(
        !outcome.replaced,
        "first register for a source_id is an insert"
    );

    // LIST mirrors the GET /adapters handler — returns store rows.
    let listed = PostgresStore::with_workspace(ws_id, || async {
        store.list_data_sources().await.expect("list")
    })
    .await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].source_id, "csv-demo");
    assert_eq!(listed[0].kind, "csv");

    // GET /adapters/{source_id} decodes the stored row through
    // `RegisterAdapterKind::from_stored`. Proves the round-trip.
    let fetched = PostgresStore::with_workspace(ws_id, || async {
        store
            .find_data_source_by_source_id("csv-demo")
            .await
            .expect("find")
    })
    .await
    .expect("row present");
    let decoded = RegisterAdapterKind::from_stored(&fetched.kind, &fetched.config)
        .expect("decode stored config");
    match decoded {
        RegisterAdapterKind::Csv {
            credential: Credential::Inline { value },
        } => {
            assert_eq!(&*value, "id,name\n1,Alice\n2,Bob\n");
        }
        _ => panic!("expected Csv + Inline credential"),
    }

    // Health: resolver count must match store count, drift lists
    // empty. Equivalent to what the /health handler reports.
    let resolver_ids: std::collections::HashSet<String> =
        match federation_state.federation_resolvers.get(&ws_id) {
            Some(slot) => match slot.get() {
                Some(lock) => lock
                    .read()
                    .await
                    .descriptions()
                    .into_iter()
                    .map(|(id, _)| id.to_string())
                    .collect(),
                None => Default::default(),
            },
            None => Default::default(),
        };
    let store_ids: std::collections::HashSet<String> =
        listed.iter().map(|row| row.source_id.clone()).collect();
    let orphans: Vec<String> = resolver_ids.difference(&store_ids).cloned().collect();
    let missing: Vec<String> = store_ids.difference(&resolver_ids).cloned().collect();
    assert!(
        orphans.is_empty() && missing.is_empty(),
        "resolver and store should be in sync post-register"
    );

    // REFRESH: refresh drops the slot, the next ensure re-hydrates.
    let rebuilt_count = PostgresStore::with_workspace(ws_id, || async {
        refresh_workspace_resolver(&federation_state, ws_id)
            .await
            .expect("refresh")
    })
    .await;
    assert_eq!(rebuilt_count, 1, "refresh should rebuild one adapter");

    // ensure_workspace_resolver should now be a cache hit and
    // return the same adapter count.
    let resolver = PostgresStore::with_workspace(ws_id, || async {
        ensure_workspace_resolver(&federation_state, ws_id)
            .await
            .expect("ensure")
    })
    .await;
    assert_eq!(resolver.len(), 1);

    // DELETE: handler removes the store row and the resolver entry.
    let removed_row = PostgresStore::with_workspace(ws_id, || async {
        store
            .delete_data_source_by_source_id("csv-demo")
            .await
            .expect("delete")
    })
    .await;
    assert!(removed_row);
    let removed_mem = remove_workspace_adapter(&federation_state, ws_id, "csv-demo").await;
    assert!(removed_mem);

    // Post-delete invariants.
    let after = PostgresStore::with_workspace(ws_id, || async {
        store.list_data_sources().await.expect("list post-delete")
    })
    .await;
    assert!(after.is_empty());
    let remaining = match federation_state.federation_resolvers.get(&ws_id) {
        Some(slot) => match slot.get() {
            Some(lock) => lock.read().await.descriptions().len(),
            None => 0,
        },
        None => 0,
    };
    assert_eq!(remaining, 0);
}

/// Concurrent upsert of the **same** `source_id` must leave store
/// and memory coherent — this is the regression guard for the race
/// the atomicity refactor closed. Ten parallel registers with
/// distinct credentials all complete successfully; the final state
/// has exactly one store row and one in-memory adapter (no
/// duplicates, no divergence).
//
// `tokio::spawn` is explicitly allowed here: the spawned tasks
// each establish their workspace context via `with_workspace`
// before touching the store, so the lint's usual concern
// (dropped WORKSPACE_ID task-local) does not apply.
#[allow(clippy::disallowed_methods)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn concurrent_upsert_same_source_id_keeps_store_and_memory_coherent() {
    let Some(url) = resolve_test_db_url() else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };

    let pg = PostgresStore::connect(&url, 16).await.expect("connect");
    pg.migrate().await.expect("migrate");
    let store: Arc<dyn ox_store::Store> = Arc::new(pg);

    let pg_for_fixtures = PostgresStore::connect(&url, 4).await.unwrap();
    pg_for_fixtures.migrate().await.unwrap();
    let ws_id = seed_workspace(&pg_for_fixtures).await;

    let federation_state = FederationState {
        store: Arc::clone(&store),
        federation_resolvers: Arc::new(DashMap::new()),
        secret_resolver: Arc::new(EnvSecretResolver),
    };
    let state = Arc::new(federation_state);

    // Spawn 10 concurrent upserts for the same source_id. Each
    // credential is slightly different so the handler-built adapter
    // differs per task — the critical section picks a winner.
    let mut handles = Vec::with_capacity(10);
    for i in 0..10 {
        let state = Arc::clone(&state);
        let handle = tokio::spawn(async move {
            let kind = RegisterAdapterKind::Csv {
                credential: Credential::Inline {
                    value: format!("id,tag\n{i},row-{i}\n").into(),
                },
            };
            PostgresStore::with_workspace(ws_id, || async {
                upsert_workspace_adapter(&state, ws_id, "contested", &kind)
                    .await
                    .expect("concurrent upsert must succeed")
            })
            .await
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Invariant 1: exactly one store row for the source_id.
    let rows = PostgresStore::with_workspace(ws_id, || async {
        store.list_data_sources().await.expect("list")
    })
    .await;
    assert_eq!(
        rows.len(),
        1,
        "10 concurrent upserts of the same source_id must leave exactly \
         one store row, got {}",
        rows.len()
    );
    assert_eq!(rows[0].source_id, "contested");

    // Invariant 2: exactly one in-memory adapter for the same.
    let slot = state
        .federation_resolvers
        .get(&ws_id)
        .expect("slot populated by upsert");
    let resolver_lock = slot.get().expect("slot hydrated");
    let descriptions = resolver_lock.read().await.descriptions();
    assert_eq!(descriptions.len(), 1);
    assert_eq!(descriptions[0].0.to_string(), "contested");
}

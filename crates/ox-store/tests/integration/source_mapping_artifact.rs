//! End-to-end integration coverage for the
//! [`SourceMappingArtifact`](ox_ontology::source_mapping::SourceMappingArtifact)
//! lifecycle (ADR 0011).
//!
//! Pins the two invariants the design pipeline depends on:
//!
//! 1. **Replay idempotency** — calling `create_artifact` twice with
//!    the same `(source_id, schema_snapshot_hash, body)` triple
//!    collapses to one row. The store's
//!    `(workspace_id, source_id, schema_snapshot_hash, content_hash)`
//!    unique constraint absorbs duplicates so a re-run of the design
//!    action against an unchanged schema does not balloon the table.
//! 2. **Schema-change diff** — a column add against the same source
//!    yields a new schema hash and therefore a new row. The previous
//!    artifact stays addressable for diff / rollback.
//!
//! Ignored by default. Run against a live PostgreSQL instance:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test integration -- --ignored integration::source_mapping_artifact
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use ox_core::source_schema::{SourceColumnDef, SourceSchema, SourceTableDef};
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::{
    CacheHintKind, ColumnRef, ObjectMappingDef, ObjectMappingId, PropertyLocation,
    PropertyMappingDef, PropertyTransform, SourceId, SourceRelationKind,
};
use ox_ontology::source_mapping::{ArtifactProvenance, SourceMappingArtifact};
use ox_ontology::test_fixtures;
use ox_store::{PostgresStore, SourceMappingArtifactStore};
use std::collections::BTreeMap;
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

async fn seed_workspace(store: &PostgresStore) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let user_email = format!("sma-test-{}@example.com", &suffix[..8]);
    let slug = format!("sma-ws-{}", &suffix[..8]);

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let provider_sub = format!("sma-test-sub-{}", &suffix[..8]);
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'SMA Test User', 'test', $2, 'designer') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('SMA Workspace', $1, $2) \
             RETURNING id",
        )
        .bind(&slug)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace")
    })
    .await
}

async fn cleanup(store: &PostgresStore, ws_id: Uuid) {
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let _ = sqlx::query("DELETE FROM source_mapping_artifacts WHERE workspace_id = $1")
            .bind(ws_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(ws_id)
            .execute(pool)
            .await;
    })
    .await;
}

async fn count_artifacts(store: &PostgresStore, source_id: &str) -> i64 {
    // SYSTEM_BYPASS lets the count see rows across every workspace
    // without seeding a 'default'. The `before_acquire` hook now
    // primes `app.workspace_id` to the nil UUID sentinel under
    // SYSTEM_BYPASS so the `ws_isolation` policy's cast still
    // succeeds (and the OR resolves through `system_bypass`).
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM source_mapping_artifacts WHERE source_id = $1",
        )
        .bind(source_id)
        .fetch_one(pool)
        .await
        .expect("count artifacts")
    })
    .await
}

fn schema_users(extra_columns: &[(&str, &str)]) -> SourceSchema {
    let mut columns = vec![SourceColumnDef {
        name: "id".into(),
        data_type: "uuid".into(),
        nullable: false,
    }];
    for (name, ty) in extra_columns {
        columns.push(SourceColumnDef {
            name: (*name).into(),
            data_type: (*ty).into(),
            nullable: true,
        });
    }
    SourceSchema {
        source_type: "postgresql".into(),
        tables: vec![SourceTableDef {
            name: "users".into(),
            columns,
            primary_key: vec!["id".into()],
        }],
        foreign_keys: vec![],
    }
}

fn ontology_with_user_node(source: &str) -> OntologyIR {
    let mut ir = test_fixtures::sample_user_ontology();
    let nt = ir.node_types()[0].id.clone();
    let om = ObjectMappingDef {
        id: ObjectMappingId::new("om-users"),
        node_type_id: nt,
        source_id: SourceId::new(source),
        relation: "users".into(),
        relation_kind: SourceRelationKind::default(),
        primary_key_columns: Vec::new(),
        partition_columns: Vec::new(),
        row_filter: None,
        property_mappings: vec![PropertyMappingDef {
            property_id: "prop-id".into(),
            property_key: ox_core::PropertyKey::new("id").unwrap(),
            location: PropertyLocation::Column(ColumnRef {
                column: "id".into(),
                relation: "users".into(),
            }),
            transform: PropertyTransform::Identity,
            concept_map_id: None,
        }],
        workspace_scope: None,
        precedence: u32::MAX,
        valid_from: None,
        valid_to: None,
        cache_hint: CacheHintKind::default(),
    };
    ir.add_object_mapping(om).expect("add_object_mapping");
    ir
}

fn provenance() -> ArtifactProvenance {
    ArtifactProvenance {
        prompt_id: "design_ontology".into(),
        prompt_version: "1.0.0".into(),
        model_id: "anthropic:claude-sonnet-4-6".into(),
        params: BTreeMap::new(),
        prompt_render_hash: String::new(),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn replay_with_unchanged_schema_collapses_to_one_row() {
    let Some(store) = connect_store().await else {
        return;
    };
    let ws_id = seed_workspace(&store).await;

    let source = format!("pg-replay-{}", &ws_id.simple().to_string()[..8]);
    let ir = ontology_with_user_node(&source);
    let schema = schema_users(&[]);

    let first = PostgresStore::with_workspace(ws_id, || async {
        store
            .create_artifact(SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new(&source),
                &schema,
                provenance(),
                "user-1",
            ))
            .await
    })
    .await
    .expect("first create_artifact");

    let second = PostgresStore::with_workspace(ws_id, || async {
        store
            .create_artifact(SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new(&source),
                &schema,
                provenance(),
                "user-1",
            ))
            .await
    })
    .await
    .expect("second create_artifact");

    assert_eq!(
        first.id, second.id,
        "content-addressed replay must return the same artifact id"
    );
    assert_eq!(
        first.schema_snapshot_hash, second.schema_snapshot_hash,
        "schema hash must be stable across replays"
    );

    let count = count_artifacts(&store, &source).await;
    assert_eq!(
        count, 1,
        "two replays of the same design must persist one row, got {count}"
    );

    cleanup(&store, ws_id).await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn schema_change_yields_a_new_row() {
    let Some(store) = connect_store().await else {
        return;
    };
    let ws_id = seed_workspace(&store).await;

    let source = format!("pg-evolve-{}", &ws_id.simple().to_string()[..8]);
    let ir = ontology_with_user_node(&source);

    let v1 = schema_users(&[]);
    let v2 = schema_users(&[("email", "text")]);
    assert_ne!(
        v1.canonical_hash(),
        v2.canonical_hash(),
        "schema versions must hash differently"
    );

    let first = PostgresStore::with_workspace(ws_id, || async {
        store
            .create_artifact(SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new(&source),
                &v1,
                provenance(),
                "user-1",
            ))
            .await
    })
    .await
    .expect("first create_artifact");

    let second = PostgresStore::with_workspace(ws_id, || async {
        store
            .create_artifact(SourceMappingArtifact::derive_from_design(
                &ir,
                &SourceId::new(&source),
                &v2,
                provenance(),
                "user-1",
            ))
            .await
    })
    .await
    .expect("second create_artifact");

    assert_ne!(
        first.id, second.id,
        "schema change must mint a new artifact id"
    );
    assert_ne!(
        first.schema_snapshot_hash, second.schema_snapshot_hash,
        "schema change must change the snapshot hash"
    );

    let count = count_artifacts(&store, &source).await;
    assert_eq!(
        count, 2,
        "design against a new schema version must add a new row, got {count}"
    );

    cleanup(&store, ws_id).await;
}

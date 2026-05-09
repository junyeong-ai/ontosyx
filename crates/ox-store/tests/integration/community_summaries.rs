//! Community-summary persistence integration tests.
//!
//! These tests exercise the Postgres-specific pieces that unit tests
//! cannot cover: RLS-bound inserts, DB-level constraints, and the
//! `WITH ORDINALITY` reverse lookup over parallel member arrays.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use chrono::Utc;
use ox_store::community::CommunitySummary;
use ox_store::{CommunitySummaryStore, PostgresStore};
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
    let user_email = format!("community-summary-{}@example.com", &suffix[..8]);
    let slug = format!("community-summary-{}", &suffix[..8]);

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let provider_sub = format!("community-summary-sub-{}", &suffix[..8]);
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'Community Summary Test User', 'test', $2, 'admin') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('Community Summary Test Workspace', $1, $2) \
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

async fn seed_ontology_version(store: &PostgresStore, workspace_id: Uuid) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();

    PostgresStore::with_workspace(workspace_id, || async {
        let pool = store.pool();
        let ontology_id: Uuid = sqlx::query_scalar(
            "INSERT INTO ontologies (lineage_id, name) \
             VALUES ($1, $2) \
             RETURNING id",
        )
        .bind(format!("community-summary-lineage-{}", &suffix[..8]))
        .bind(format!("community-summary-ontology-{}", &suffix[..8]))
        .fetch_one(pool)
        .await
        .expect("insert ontology");

        sqlx::query_scalar(
            "INSERT INTO ontology_version_snapshots \
                (ontology_id, version, committed_by, commit_message) \
             VALUES ($1, '1', 'community-summary-test', 'seed') \
             RETURNING id",
        )
        .bind(ontology_id)
        .fetch_one(pool)
        .await
        .expect("insert ontology version")
    })
    .await
}

fn summary(
    workspace_id: Uuid,
    ontology_version_id: Uuid,
    community_id: &str,
    member_entity_kinds: Vec<&str>,
    member_logical_ids: Vec<&str>,
) -> CommunitySummary {
    let kinds: Vec<String> = member_entity_kinds
        .into_iter()
        .map(ToString::to_string)
        .collect();
    let logical: Vec<String> = member_logical_ids
        .into_iter()
        .map(ToString::to_string)
        .collect();
    let fingerprint = CommunitySummary::compute_member_fingerprint(&kinds, &logical);
    CommunitySummary {
        id: Uuid::now_v7(),
        workspace_id,
        ontology_version_id,
        community_id: community_id.to_string(),
        level: 1,
        member_entity_kinds: kinds,
        member_logical_ids: logical,
        member_fingerprint: fingerprint,
        title: format!("{community_id} summary"),
        summary: format!("{community_id} retrieval summary for GraphRAG context."),
        tokenized_text: String::new(),
        tokenizer_dict_fingerprint: String::new(),
        embedding: None,
        generated_at: Utc::now(),
    }
}

#[tokio::test]
#[ignore]
async fn reverse_lookup_requires_member_kind_and_id_at_same_index() {
    let Some(store) = connect_store().await else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let workspace_id = seed_workspace(&store).await;
    let version_id = seed_ontology_version(&store, workspace_id).await;

    PostgresStore::with_workspace(workspace_id, || async {
        store
            .upsert_community_summary(&summary(
                workspace_id,
                version_id,
                "mixed-index",
                vec!["Concept", "NodeType"],
                vec!["c_vip", "nt_customer"],
            ))
            .await
            .expect("upsert mixed-index summary");
        store
            .upsert_community_summary(&summary(
                workspace_id,
                version_id,
                "node-only",
                vec!["NodeType"],
                vec!["nt_customer"],
            ))
            .await
            .expect("upsert node-only summary");

        let false_positive = store
            .list_communities_for_entity(version_id, "NodeType", "c_vip")
            .await
            .expect("reverse lookup false-positive guard");
        assert!(
            false_positive.is_empty(),
            "kind/id matches from different member indexes must not count"
        );

        let matching = store
            .list_communities_for_entity(version_id, "NodeType", "nt_customer")
            .await
            .expect("reverse lookup matching entity");
        let community_ids: Vec<_> = matching
            .iter()
            .map(|row| row.community_id.as_str())
            .collect();
        assert_eq!(community_ids, vec!["mixed-index", "node-only"]);
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn database_rejects_invalid_parallel_member_arrays() {
    let Some(store) = connect_store().await else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let workspace_id = seed_workspace(&store).await;
    let version_id = seed_ontology_version(&store, workspace_id).await;

    PostgresStore::with_workspace(workspace_id, || async {
        let result = sqlx::query(
            "INSERT INTO ontology_community_summaries
                (ontology_version_id, community_id, level, member_entity_kinds,
                 member_logical_ids, title, summary)
             VALUES ($1, 'invalid-parallel-arrays', 0, ARRAY['NodeType', 'Concept']::text[],
                     ARRAY['nt_customer']::text[], 'Invalid', 'Invalid')",
        )
        .bind(version_id)
        .execute(store.pool())
        .await;

        assert!(
            result.is_err(),
            "database constraints must reject misaligned member arrays"
        );
    })
    .await;
}

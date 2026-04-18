//! Integration tests for migration `0007_i18n_and_ir_evolution`.
//!
//! These tests run against a live PostgreSQL instance configured via
//! `OX_TEST_DATABASE_URL` (or the `OX_DATABASE_URL` / `DATABASE_URL` fallbacks).
//! They are marked `#[ignore]` by default so `cargo test` succeeds in
//! environments without a database. Run them explicitly with:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test migration_0007 -- --ignored
//! ```
//!
//! They cover:
//! 1. Schema additions: `workspaces.primary_locale` + `workspaces.locale_fallback`.
//! 2. Constraint enforcement: BCP 47 shape on the locale columns.
//! 3. Lifecycle of helper functions: validator survives, converters dropped.
//! 4. Idempotency of the JSONB conversion helpers when re-applied to data
//!    that has already been migrated (the rollforward safety net).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

/// Resolve the test database URL from the conventional env vars.
/// Returns `None` if no URL is configured — the caller should skip silently.
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

async fn connect_pool() -> Option<PgPool> {
    let url = resolve_test_db_url()?;
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
        .ok()
}

/// Apply the workspace's full migration set to the connected pool. Idempotent;
/// sqlx tracks applied migrations and skips ones already recorded in
/// `_sqlx_migrations`.
async fn apply_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("workspace migrations should apply cleanly");
}

// ---------------------------------------------------------------------------
// 1. Schema verification — columns + defaults
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn workspace_locale_columns_exist_with_canonical_defaults() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    apply_migrations(&pool).await;

    let row = sqlx::query(
        "SELECT column_name, data_type, column_default \
           FROM information_schema.columns \
          WHERE table_name = 'workspaces' \
            AND column_name IN ('primary_locale', 'locale_fallback') \
       ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query columns");

    assert_eq!(
        row.len(),
        2,
        "expected primary_locale + locale_fallback columns to exist"
    );

    // The migration sets `'ko'` as primary and `["ko","en"]` as fallback —
    // sane defaults for Korean-first deployments. New workspaces created
    // without explicit locale should inherit these.
    let mut by_name = std::collections::HashMap::new();
    for r in &row {
        let name: String = r.try_get("column_name").unwrap();
        let default: Option<String> = r.try_get("column_default").unwrap();
        by_name.insert(name, default);
    }
    let primary = by_name
        .get("primary_locale")
        .and_then(|d| d.clone())
        .unwrap_or_default();
    assert!(
        primary.contains("'ko'"),
        "primary_locale default should be 'ko', got: {primary}"
    );
    let fallback = by_name
        .get("locale_fallback")
        .and_then(|d| d.clone())
        .unwrap_or_default();
    assert!(
        fallback.contains("ko") && fallback.contains("en"),
        "locale_fallback default should include ko and en, got: {fallback}"
    );
}

// ---------------------------------------------------------------------------
// 2. Helper function lifecycle — validator survives, converters dropped
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn validator_helper_survives_and_converter_helpers_are_dropped() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    apply_migrations(&pool).await;

    let exists = |name: &'static str, pool: PgPool| async move {
        let row = sqlx::query(
            "SELECT EXISTS ( \
                 SELECT 1 FROM pg_proc p \
                   JOIN pg_namespace n ON n.oid = p.pronamespace \
                  WHERE p.proname = $1 AND n.nspname = 'public' \
             ) AS present",
        )
        .bind(name)
        .fetch_one(&pool)
        .await
        .expect("query pg_proc");
        row.try_get::<bool, _>("present").unwrap()
    };

    assert!(
        exists("fn_validate_locale_chain", pool.clone()).await,
        "fn_validate_locale_chain must remain — workspaces CHECK depends on it"
    );
    assert!(
        !exists("fn_to_localized_text", pool.clone()).await,
        "fn_to_localized_text should have been dropped at end of migration"
    );
    assert!(
        !exists("fn_to_ontology_version", pool.clone()).await,
        "fn_to_ontology_version should have been dropped at end of migration"
    );
    assert!(
        !exists("fn_migrate_ontology", pool.clone()).await,
        "fn_migrate_ontology should have been dropped at end of migration"
    );
}

// ---------------------------------------------------------------------------
// 3. Locale validator — accepts BCP 47, rejects malformed
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn locale_chain_validator_enforces_bcp47_shape() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    apply_migrations(&pool).await;

    let cases: &[(serde_json::Value, bool)] = &[
        // Accepted shapes
        (json!(["ko"]), true),
        (json!(["ko", "en"]), true),
        (json!(["zh-hant", "zh-hant-tw"]), true),
        (json!(["en-us"]), true),
        // Rejected shapes
        (json!([]), false),                 // empty array
        (json!(["KO"]), false),             // uppercase
        (json!(["ko", 42]), false),         // non-string element
        (json!("ko"), false),               // not an array
        (json!(["ko-LongerThan8"]), false), // subtag too long
    ];

    for (input, expected) in cases {
        let row = sqlx::query("SELECT fn_validate_locale_chain($1::jsonb) AS ok")
            .bind(input)
            .fetch_one(&pool)
            .await
            .expect("invoke fn_validate_locale_chain");
        let actual: bool = row.try_get("ok").unwrap();
        assert_eq!(
            actual, *expected,
            "fn_validate_locale_chain({input:?}) returned {actual}, expected {expected}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. JSONB conversion idempotency — rollforward safety net
//
// The migration drops the helper conversion functions at the end. To verify
// their idempotency, the test re-defines them inside a transaction (matching
// the migration's bodies verbatim) and asserts that applying them twice to
// already-canonical data produces no further change.
// ---------------------------------------------------------------------------

const LEGACY_CONVERTER_HELPERS: &str = r#"
CREATE OR REPLACE FUNCTION fn_to_localized_text(v jsonb) RETURNS jsonb AS $$
BEGIN
    IF v IS NULL OR v = 'null'::jsonb THEN
        RETURN jsonb_build_object('default', '', 'translations', '{}'::jsonb);
    END IF;
    IF jsonb_typeof(v) = 'string' THEN
        RETURN jsonb_build_object(
            'default', v #>> '{}',
            'translations', '{}'::jsonb
        );
    END IF;
    IF jsonb_typeof(v) = 'object' THEN
        RETURN jsonb_build_object(
            'default', COALESCE(v -> 'default', '""'::jsonb) #>> '{}',
            'translations', COALESCE(v -> 'translations', '{}'::jsonb)
        );
    END IF;
    RETURN jsonb_build_object('default', '', 'translations', '{}'::jsonb);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION fn_to_ontology_version(v jsonb) RETURNS jsonb AS $$
BEGIN
    IF v IS NULL OR v = 'null'::jsonb THEN
        RETURN jsonb_build_object('number', 1);
    END IF;
    IF jsonb_typeof(v) = 'number' THEN
        RETURN jsonb_build_object('number', (v #>> '{}')::int);
    END IF;
    IF jsonb_typeof(v) = 'object' THEN
        RETURN v;
    END IF;
    RETURN jsonb_build_object('number', 1);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION fn_migrate_ontology(ont jsonb) RETURNS jsonb AS $$
DECLARE
    out_ont jsonb;
    out_nodes jsonb := '[]'::jsonb;
    out_edges jsonb := '[]'::jsonb;
    node jsonb;
    edge jsonb;
    out_props jsonb;
    prop jsonb;
BEGIN
    IF ont IS NULL THEN
        RETURN NULL;
    END IF;
    IF jsonb_typeof(ont) <> 'object' THEN
        RETURN ont;
    END IF;

    out_ont := ont;
    out_ont := jsonb_set(out_ont, '{description}', fn_to_localized_text(ont -> 'description'), true);
    out_ont := jsonb_set(out_ont, '{version}', fn_to_ontology_version(ont -> 'version'), true);

    IF ont ? 'node_types' THEN
        FOR node IN SELECT value FROM jsonb_array_elements(ont -> 'node_types') LOOP
            node := jsonb_set(node, '{description}', fn_to_localized_text(node -> 'description'), true);
            node := node - 'source_table';
            IF node ? 'properties' THEN
                out_props := '[]'::jsonb;
                FOR prop IN SELECT value FROM jsonb_array_elements(node -> 'properties') LOOP
                    prop := jsonb_set(prop, '{description}', fn_to_localized_text(prop -> 'description'), true);
                    out_props := out_props || jsonb_build_array(prop);
                END LOOP;
                node := jsonb_set(node, '{properties}', out_props, true);
            END IF;
            out_nodes := out_nodes || jsonb_build_array(node);
        END LOOP;
        out_ont := jsonb_set(out_ont, '{node_types}', out_nodes, true);
    END IF;

    IF ont ? 'edge_types' THEN
        FOR edge IN SELECT value FROM jsonb_array_elements(ont -> 'edge_types') LOOP
            edge := jsonb_set(edge, '{description}', fn_to_localized_text(edge -> 'description'), true);
            IF edge ? 'properties' THEN
                out_props := '[]'::jsonb;
                FOR prop IN SELECT value FROM jsonb_array_elements(edge -> 'properties') LOOP
                    prop := jsonb_set(prop, '{description}', fn_to_localized_text(prop -> 'description'), true);
                    out_props := out_props || jsonb_build_array(prop);
                END LOOP;
                edge := jsonb_set(edge, '{properties}', out_props, true);
            END IF;
            out_edges := out_edges || jsonb_build_array(edge);
        END LOOP;
        out_ont := jsonb_set(out_ont, '{edge_types}', out_edges, true);
    END IF;

    RETURN out_ont;
END;
$$ LANGUAGE plpgsql IMMUTABLE;
"#;

#[tokio::test]
#[ignore]
async fn fn_to_localized_text_normalises_legacy_shapes() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    apply_migrations(&pool).await;
    let mut tx = pool.begin().await.expect("begin tx");
    sqlx::raw_sql(LEGACY_CONVERTER_HELPERS)
        .execute(&mut *tx)
        .await
        .expect("recreate helpers");

    let cases: &[(serde_json::Value, serde_json::Value)] = &[
        // null → empty LocalizedText
        (
            json!(null),
            json!({"default": "", "translations": {}}),
        ),
        // bare string → default-only LocalizedText
        (
            json!("Hello"),
            json!({"default": "Hello", "translations": {}}),
        ),
        // already-shape → preserved
        (
            json!({"default": "안녕", "translations": {"en": "Hi"}}),
            json!({"default": "안녕", "translations": {"en": "Hi"}}),
        ),
        // empty object → empty LocalizedText
        (
            json!({}),
            json!({"default": "", "translations": {}}),
        ),
    ];

    for (input, expected) in cases {
        let row = sqlx::query("SELECT fn_to_localized_text($1::jsonb) AS out")
            .bind(input)
            .fetch_one(&mut *tx)
            .await
            .expect("invoke fn_to_localized_text");
        let actual: serde_json::Value = row.try_get("out").unwrap();
        assert_eq!(
            &actual, expected,
            "fn_to_localized_text({input:?}) = {actual:?}, expected {expected:?}"
        );
    }

    // Idempotency: apply twice → same result as once
    let legacy = json!("hello");
    let once: serde_json::Value =
        sqlx::query("SELECT fn_to_localized_text($1::jsonb) AS out")
            .bind(&legacy)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
            .try_get("out")
            .unwrap();
    let twice: serde_json::Value =
        sqlx::query("SELECT fn_to_localized_text(fn_to_localized_text($1::jsonb)) AS out")
            .bind(&legacy)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
            .try_get("out")
            .unwrap();
    assert_eq!(
        once, twice,
        "fn_to_localized_text must be idempotent under repeated application"
    );

    tx.rollback().await.expect("rollback");
}

#[tokio::test]
#[ignore]
async fn fn_migrate_ontology_is_idempotent_on_legacy_and_canonical_shapes() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    apply_migrations(&pool).await;
    let mut tx = pool.begin().await.expect("begin tx");
    sqlx::raw_sql(LEGACY_CONVERTER_HELPERS)
        .execute(&mut *tx)
        .await
        .expect("recreate helpers");

    // A pre-Phase-A ontology document: scalar version, scalar descriptions,
    // and a node carrying the now-removed `source_table` field.
    let legacy = json!({
        "id": "ont-1",
        "name": "Sales",
        "description": "사용자 매출 도메인",
        "version": 7,
        "node_types": [{
            "id": "n-customer",
            "label": "Customer",
            "description": "구매자",
            "source_table": "customers",
            "properties": [{
                "id": "p-name",
                "name": "name",
                "description": "고객 이름",
                "property_type": "String",
                "nullable": false
            }]
        }],
        "edge_types": [{
            "id": "e-bought",
            "label": "BOUGHT",
            "description": "구매 관계",
            "source_node_id": "n-customer",
            "target_node_id": "n-product",
            "cardinality": "ManyToMany",
            "properties": []
        }]
    });

    let once: serde_json::Value = sqlx::query("SELECT fn_migrate_ontology($1::jsonb) AS out")
        .bind(&legacy)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .try_get("out")
        .unwrap();

    // Top-level conversions
    assert_eq!(
        once["description"],
        json!({"default": "사용자 매출 도메인", "translations": {}}),
        "top-level description should become LocalizedText"
    );
    assert_eq!(
        once["version"],
        json!({"number": 7}),
        "scalar version should become OntologyVersion object"
    );

    // Node conversions
    let node = &once["node_types"][0];
    assert!(
        node.get("source_table").is_none(),
        "legacy source_table field must be stripped"
    );
    assert_eq!(
        node["description"],
        json!({"default": "구매자", "translations": {}})
    );
    assert_eq!(
        node["properties"][0]["description"],
        json!({"default": "고객 이름", "translations": {}})
    );

    // Edge conversion
    let edge = &once["edge_types"][0];
    assert_eq!(
        edge["description"],
        json!({"default": "구매 관계", "translations": {}})
    );

    // Idempotency: re-running the migration on already-canonical data
    // produces an identical document.
    let twice: serde_json::Value = sqlx::query("SELECT fn_migrate_ontology($1::jsonb) AS out")
        .bind(&once)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .try_get("out")
        .unwrap();
    assert_eq!(
        once, twice,
        "fn_migrate_ontology must be a no-op when applied to already-canonical data"
    );

    // Triple-application: still equivalent. Catches subtle accumulators.
    let thrice: serde_json::Value = sqlx::query("SELECT fn_migrate_ontology($1::jsonb) AS out")
        .bind(&twice)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .try_get("out")
        .unwrap();
    assert_eq!(
        once, thrice,
        "fn_migrate_ontology must be stable under repeated application"
    );

    tx.rollback().await.expect("rollback");
}

#[tokio::test]
#[ignore]
async fn fn_to_ontology_version_normalises_scalar_and_object_inputs() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    apply_migrations(&pool).await;
    let mut tx = pool.begin().await.expect("begin tx");
    sqlx::raw_sql(LEGACY_CONVERTER_HELPERS)
        .execute(&mut *tx)
        .await
        .expect("recreate helpers");

    let cases: &[(serde_json::Value, serde_json::Value)] = &[
        (json!(null), json!({"number": 1})),
        (json!(3), json!({"number": 3})),
        // already-shape → preserved exactly (including extra fields)
        (
            json!({"number": 5, "valid_from": "2026-01-01T00:00:00Z"}),
            json!({"number": 5, "valid_from": "2026-01-01T00:00:00Z"}),
        ),
    ];

    for (input, expected) in cases {
        let row = sqlx::query("SELECT fn_to_ontology_version($1::jsonb) AS out")
            .bind(input)
            .fetch_one(&mut *tx)
            .await
            .expect("invoke fn_to_ontology_version");
        let actual: serde_json::Value = row.try_get("out").unwrap();
        assert_eq!(
            &actual, expected,
            "fn_to_ontology_version({input:?}) = {actual:?}, expected {expected:?}"
        );
    }

    tx.rollback().await.expect("rollback");
}

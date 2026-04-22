//! PostgreSQL adapter `scan()` integration test.
//!
//! Exercises `PostgresAdapter::scan` against a live Postgres. The
//! test creates a temporary table, inserts a few rows, reads them
//! back through `scan`, and asserts the Arrow types + values match.
//!
//! Ignored by default — run with a live database:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-source --test postgres_scan_integration -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use arrow_array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};
use ox_source::DataSourceAdapter;
use ox_source::postgres::PostgresAdapter;
use sqlx::Executor;

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

#[tokio::test]
#[ignore]
async fn postgres_scan_returns_typed_record_batch() {
    let Some(url) = resolve_test_db_url() else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let adapter = PostgresAdapter::connect(&url, "public").await.unwrap();
    let pool = adapter.pool();

    // Use a suffix that's unique per test run to avoid collisions on
    // concurrent CI shards.
    let table = format!(
        "ox_scan_test_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let qualified = format!("\"{table}\"");

    pool.execute(
        format!(
            "CREATE TABLE {qualified} (
                id bigint NOT NULL,
                name text,
                amount double precision,
                active boolean
            )"
        )
        .as_str(),
    )
    .await
    .expect("create table");

    pool.execute(
        format!(
            "INSERT INTO {qualified} (id, name, amount, active) VALUES
                (1, 'Alice', 100.5, TRUE),
                (2, 'Bob', 42.0, FALSE),
                (3, NULL, NULL, NULL)"
        )
        .as_str(),
    )
    .await
    .expect("insert rows");

    let batch = adapter.scan(&table, None, None).await.expect("scan");
    // Cleanup runs regardless of assertion outcome so a failed test
    // does not leave stray tables behind.
    let cleanup = format!("DROP TABLE {qualified}");
    let _ = pool.execute(cleanup.as_str()).await;

    assert_eq!(batch.num_rows(), 3, "3 inserted rows round-trip");
    assert_eq!(batch.num_columns(), 4);

    // Column 0: id (bigint → Int64)
    let ids = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("id column is Int64");
    assert_eq!(ids.value(0), 1);
    assert_eq!(ids.value(1), 2);
    assert_eq!(ids.value(2), 3);

    // Column 1: name (text → Utf8, nullable row 3)
    let names = batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("name column is Utf8");
    assert_eq!(names.value(0), "Alice");
    assert_eq!(names.value(1), "Bob");
    assert!(names.is_null(2));

    // Column 2: amount (double → Float64, nullable row 3)
    let amounts = batch
        .column(2)
        .as_any()
        .downcast_ref::<Float64Array>()
        .expect("amount column is Float64");
    assert!((amounts.value(0) - 100.5).abs() < 1e-9);
    assert!((amounts.value(1) - 42.0).abs() < 1e-9);
    assert!(amounts.is_null(2));

    // Column 3: active (boolean, nullable row 3)
    let actives = batch
        .column(3)
        .as_any()
        .downcast_ref::<BooleanArray>()
        .expect("active column is Boolean");
    assert!(actives.value(0));
    assert!(!actives.value(1));
    assert!(actives.is_null(2));
}

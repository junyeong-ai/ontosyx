//! BigQuery adapter live integration tests.
//!
//! Exercises the six read-only `DataSourceAdapter` primitives against a
//! real BigQuery dataset:
//!
//! 1. `list_tables_with_summary`
//! 2. `describe_table`
//! 3. `count_rows`
//! 4. `sample_column`
//! 5. `scan(limit=10)`
//! 6. `list_foreign_keys`
//!
//! ## Running
//!
//! Compiled out by default. Enable the cargo feature to opt in:
//!
//! ```sh
//! OXY_BIGQUERY_PROJECT=oydp-public-dw \
//! OXY_BIGQUERY_DATASET=dim \
//! OXY_BIGQUERY_BILLING_PROJECT=oy-dwusers \
//!     cargo test -p ox-source --features bigquery-integration-tests \
//!         --test bigquery_integration
//! ```
//!
//! `OXY_BIGQUERY_BILLING_PROJECT` is optional — set it when the runner
//! has `bigquery.tables.list` on the data project but lacks
//! `bigquery.jobs.create` there (typical for shared analytics
//! datasets), or when a VPC Service Controls perimeter forces jobs to
//! run from a particular project.
//!
//! Authentication uses Application Default Credentials. The adapter
//! accepts the standard gcloud authorized-user file written by
//! `gcloud auth application-default login`, or any service-account
//! JSON pointed at by `GOOGLE_APPLICATION_CREDENTIALS`.
//!
//! ## CI escalation
//!
//! Set `OXY_REQUIRE_BIGQUERY_TESTS=true` to convert the silent skip
//! (missing project / dataset) into a hard failure. Used in CI shards
//! that have workload-identity wired up.
//!
//! ## Table selection
//!
//! Deterministic: every test fetches `list_tables` and picks the first
//! entry after sorting alphabetically. No "smallest" / "fewest columns"
//! heuristics — the choice has to be reproducible across regions, GCP
//! projects, and reruns.

#![cfg(feature = "bigquery-integration-tests")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use ox_source::DataSourceAdapter;
use ox_source::bigquery::BigQueryAdapter;

const PROJECT_ENV: &str = "OXY_BIGQUERY_PROJECT";
const DATASET_ENV: &str = "OXY_BIGQUERY_DATASET";
const BILLING_PROJECT_ENV: &str = "OXY_BIGQUERY_BILLING_PROJECT";
const REQUIRE_ENV: &str = "OXY_REQUIRE_BIGQUERY_TESTS";

/// Resolve project + dataset from env, or honour the CI escalation flag.
///
/// Returns `Some((project, dataset))` when the env is set,
/// `None` when the test should silently skip (developer machine
/// without credentials), and `panic!` when `OXY_REQUIRE_BIGQUERY_TESTS=true`
/// but the project / dataset is missing — that combination indicates a
/// CI misconfiguration we want loud, not silent.
fn resolve_target() -> Option<(String, String)> {
    let project = std::env::var(PROJECT_ENV).ok().filter(|s| !s.is_empty());
    let dataset = std::env::var(DATASET_ENV).ok().filter(|s| !s.is_empty());
    let required = std::env::var(REQUIRE_ENV)
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);

    match (project, dataset) {
        (Some(p), Some(d)) => Some((p, d)),
        _ if required => {
            panic!(
                "{REQUIRE_ENV}=true but {PROJECT_ENV} / {DATASET_ENV} is missing — \
                 BigQuery integration tests cannot run"
            );
        }
        _ => {
            eprintln!(
                "{PROJECT_ENV} / {DATASET_ENV} not set — skipping BigQuery integration test"
            );
            None
        }
    }
}

async fn connect() -> Option<BigQueryAdapter> {
    let (project, dataset) = resolve_target()?;
    let billing = std::env::var(BILLING_PROJECT_ENV)
        .ok()
        .filter(|s| !s.is_empty());
    let uri = match billing {
        Some(billing) => format!("bigquery://{project}/{dataset}?billing_project_id={billing}"),
        None => format!("bigquery://{project}/{dataset}"),
    };
    let adapter = BigQueryAdapter::connect(&uri)
        .await
        .expect("BigQuery ADC authentication");
    Some(adapter)
}

/// Pick the alphabetically first table the dataset advertises.
/// Returns `None` when the dataset is empty — caller decides whether
/// to skip or fail.
async fn first_table(adapter: &BigQueryAdapter) -> Option<String> {
    let mut tables = adapter.list_tables().await.expect("list_tables");
    tables.sort();
    tables.into_iter().next()
}

#[tokio::test]
async fn list_tables_with_summary_returns_metadata() {
    let Some(adapter) = connect().await else {
        return;
    };
    let summaries = adapter
        .list_tables_with_summary()
        .await
        .expect("list_tables_with_summary");

    assert!(
        !summaries.is_empty(),
        "expected at least one table in the configured dataset"
    );
    for s in &summaries {
        assert!(!s.name.is_empty(), "every summary carries a table name");
        // `column_count` is u32 — a table with zero columns would
        // still be valid metadata, so no positive-count assertion.
    }
}

#[tokio::test]
async fn describe_table_returns_columns() {
    let Some(adapter) = connect().await else {
        return;
    };
    let Some(table) = first_table(&adapter).await else {
        eprintln!("dataset is empty — skipping describe_table");
        return;
    };
    let described = adapter.describe_table(&table).await.expect("describe_table");

    assert_eq!(described.name, table);
    assert!(
        !described.columns.is_empty(),
        "described table should have at least one column"
    );
    for c in &described.columns {
        assert!(!c.name.is_empty(), "every column has a name");
        assert!(
            !c.data_type.is_empty(),
            "every column has a non-empty raw data_type"
        );
    }
}

#[tokio::test]
async fn count_rows_returns_non_negative() {
    let Some(adapter) = connect().await else {
        return;
    };
    let Some(table) = first_table(&adapter).await else {
        eprintln!("dataset is empty — skipping count_rows");
        return;
    };
    // u64 cannot be negative — the test is mostly that this completes
    // without error and doesn't blow past the metadata fast path.
    let _ = adapter.count_rows(&table).await.expect("count_rows");
}

#[tokio::test]
async fn sample_column_returns_stats_for_first_column() {
    let Some(adapter) = connect().await else {
        return;
    };
    let Some(table) = first_table(&adapter).await else {
        eprintln!("dataset is empty — skipping sample_column");
        return;
    };
    let described = adapter.describe_table(&table).await.expect("describe_table");
    let Some(column) = described.columns.first() else {
        eprintln!("table {table} has no columns — skipping sample_column");
        return;
    };
    let stats = adapter
        .sample_column(&table, column)
        .await
        .expect("sample_column");
    assert_eq!(stats.column_name, column.name);
}

#[tokio::test]
async fn scan_with_limit_returns_record_batch() {
    let Some(adapter) = connect().await else {
        return;
    };
    let Some(table) = first_table(&adapter).await else {
        eprintln!("dataset is empty — skipping scan");
        return;
    };
    let batch = adapter
        .scan(&table, None, Some(10))
        .await
        .expect("scan with limit");
    assert!(
        batch.num_rows() <= 10,
        "scan(limit=10) returned {} rows",
        batch.num_rows()
    );
    let described = adapter.describe_table(&table).await.expect("describe_table");
    assert_eq!(
        batch.num_columns(),
        described.columns.len(),
        "scan returns one column per described column"
    );
}

#[tokio::test]
async fn list_foreign_keys_succeeds() {
    let Some(adapter) = connect().await else {
        return;
    };
    // BigQuery FK declarations are informational and rarely populated.
    // The contract is "no error" — an empty result is legitimate.
    let _ = adapter
        .list_foreign_keys()
        .await
        .expect("list_foreign_keys");
}

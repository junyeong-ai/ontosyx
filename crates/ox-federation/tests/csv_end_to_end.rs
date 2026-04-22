//! End-to-end federation test: CSV source → DataFusion SELECT.
//!
//! Smoke-level proof that the Phase 2 plumbing is wired:
//! `CsvAdapter` → `SourceTableProvider` → `FederationContext::run_sql`
//! returns rows the calling SQL selected.

use std::sync::Arc;

use ox_federation::{FederationContext, SourceTableProvider, context::WorkspaceRef};
use ox_source::DataSourceAdapter;
use ox_source::sample::CsvAdapter;

fn test_csv() -> &'static str {
    "id,name,amount\n\
     1,Alice,100.5\n\
     2,Bob,250.0\n\
     3,Charlie,42.25\n\
     4,Dana,88.0\n"
}

#[tokio::test]
async fn select_star_from_registered_csv_returns_every_row() {
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(test_csv()).unwrap());
    let provider = SourceTableProvider::try_new(adapter, "records")
        .await
        .unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    ctx.register_table(Arc::new(provider)).unwrap();

    let batches = ctx.run_sql("SELECT id, name, amount FROM records").await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 4);
    // Column count after projection.
    assert_eq!(batches[0].num_columns(), 3);
}

#[tokio::test]
async fn where_filter_runs_engine_side_on_csv_source() {
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(test_csv()).unwrap());
    let provider = SourceTableProvider::try_new(adapter, "records")
        .await
        .unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    ctx.register_table(Arc::new(provider)).unwrap();

    let batches = ctx
        .run_sql("SELECT name FROM records WHERE amount > 50 ORDER BY name")
        .await
        .unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    // Alice (100.5), Bob (250.0), Dana (88.0) → 3 rows.
    assert_eq!(total_rows, 3);
}

#[tokio::test]
async fn select_with_limit_truncates_results() {
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(test_csv()).unwrap());
    let provider = SourceTableProvider::try_new(adapter, "records")
        .await
        .unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    ctx.register_table(Arc::new(provider)).unwrap();

    let batches = ctx.run_sql("SELECT id FROM records LIMIT 2").await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 2);
}

#[tokio::test]
async fn projection_only_returns_requested_columns() {
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(test_csv()).unwrap());
    let provider = SourceTableProvider::try_new(adapter, "records")
        .await
        .unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    ctx.register_table(Arc::new(provider)).unwrap();

    let batches = ctx.run_sql("SELECT name FROM records").await.unwrap();
    assert!(!batches.is_empty());
    assert_eq!(batches[0].num_columns(), 1);
    assert_eq!(batches[0].schema().field(0).name(), "name");
}

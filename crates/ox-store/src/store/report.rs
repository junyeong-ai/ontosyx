//! Parameterized saved reports — Cypher templates with bind variables.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::SavedReport;

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait ReportStore: Send + Sync {
    async fn create_report(&self, report: &SavedReport) -> OxResult<()>;
    async fn get_report(&self, id: Uuid) -> OxResult<Option<SavedReport>>;
    async fn list_reports(
        &self,
        user_id: &str,
        ontology_lineage_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedReport>>;
    async fn update_report(
        &self,
        id: Uuid,
        title: &str,
        description: Option<&str>,
        query_template: &str,
        parameters: &serde_json::Value,
        widget_type: Option<&str>,
        is_public: bool,
    ) -> OxResult<()>;
    async fn delete_report(&self, id: Uuid) -> OxResult<bool>;
}

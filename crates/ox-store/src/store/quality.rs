//! Declarative data-quality rules with evaluation results.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{QualityDashboardEntry, QualityResult, QualityRule};

#[async_trait]
pub trait QualityStore: Send + Sync {
    async fn create_quality_rule(&self, rule: &QualityRule) -> OxResult<()>;
    async fn get_quality_rule(&self, id: Uuid) -> OxResult<Option<QualityRule>>;
    async fn list_quality_rules(
        &self,
        ontology_lineage_id: Option<&str>,
        target_label: Option<&str>,
    ) -> OxResult<Vec<QualityRule>>;
    async fn update_quality_rule(
        &self,
        id: Uuid,
        name: &str,
        threshold: f64,
        is_active: bool,
    ) -> OxResult<()>;
    async fn delete_quality_rule(&self, id: Uuid) -> OxResult<bool>;
    async fn record_quality_result(&self, result: &QualityResult) -> OxResult<()>;
    async fn list_latest_results(&self, rule_id: Uuid, limit: i64) -> OxResult<Vec<QualityResult>>;
    async fn list_quality_dashboard_entries(&self) -> OxResult<Vec<QualityDashboardEntry>>;
}

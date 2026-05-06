//! Workspace-wide PROV-O audit records.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::error::OxResult;

use super::CursorPage;

/// Free-form filter applied to the audit endpoint. Every field is
/// optional; an empty filter returns the full workspace stream.
#[derive(Debug, Clone, Default)]
pub struct AuditTrailFilter {
    pub ontology_id: Option<Uuid>,
    pub activity_kind: Option<String>,
    pub agent_kind: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// One record in the audit stream. The `provenance` payload is the
/// content-addressed PROV-O entity (`ProvenanceDef`) emitted at IR
/// commit time; the surrounding fields attribute it to the source
/// ontology so a multi-ontology workspace can render a rolled-up
/// view without an extra detail fetch.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct AuditRecord {
    pub ontology_id: Uuid,
    pub ontology_lineage_id: String,
    pub ontology_name: String,
    pub provenance: serde_json::Value,
    pub at_time: DateTime<Utc>,
}

#[async_trait]
pub trait AuditTrailStore: Send + Sync {
    /// Stream PROV-O records across every committed ontology in the
    /// workspace, filtered + cursor-paginated. Ordering is `at_time`
    /// descending with the entity hash as the deterministic tiebreak.
    async fn list_audit_records(
        &self,
        filter: AuditTrailFilter,
        cursor: Option<&str>,
        limit: i64,
    ) -> OxResult<CursorPage<AuditRecord>>;
}

//! [`ProvenanceStore`] postgres impl.
//!
//! Backs `provenance_records` from the schema baseline. Each row
//! mirrors a `ProvenanceDef` — Activity / Agent / subject DAG plus
//! the optional Plan reference (template id + version + prompt
//! render hash) for LLM activities.
//!
//! All workspace-scoped via the bound task-local — writes carry
//! the active `WORKSPACE_ID`, reads filter via the row-level
//! `ws_isolation` policy. Cross-tenant ids resolve to `None` on
//! lookup.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{
    AgentRef, EntityRef, ProvenanceActivityKind, ProvenanceCapture, ProvenanceDef, ProvenanceId,
    ProvenancePlan,
};

use crate::store::ProvenanceStore;

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct ProvenanceRow {
    id: Uuid,
    subject: serde_json::Value,
    activity: serde_json::Value,
    agent: serde_json::Value,
    plan: Option<serde_json::Value>,
    used: serde_json::Value,
    derived_from: serde_json::Value,
    was_informed_by: Vec<Uuid>,
    at_time: chrono::DateTime<chrono::Utc>,
    ontology_valid_at: Option<chrono::DateTime<chrono::Utc>>,
    data_valid_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ProvenanceRow {
    fn into_def(self) -> OxResult<ProvenanceDef> {
        let subject: EntityRef =
            serde_json::from_value(self.subject).map_err(|e| OxError::Runtime {
                message: format!("decode provenance_records.subject failed: {e}"),
            })?;
        let activity: ProvenanceActivityKind =
            serde_json::from_value(self.activity).map_err(|e| OxError::Runtime {
                message: format!("decode provenance_records.activity failed: {e}"),
            })?;
        let agent: AgentRef = serde_json::from_value(self.agent).map_err(|e| OxError::Runtime {
            message: format!("decode provenance_records.agent failed: {e}"),
        })?;
        let plan: Option<ProvenancePlan> = match self.plan {
            Some(v) => Some(serde_json::from_value(v).map_err(|e| OxError::Runtime {
                message: format!("decode provenance_records.plan failed: {e}"),
            })?),
            None => None,
        };
        let used: Vec<EntityRef> =
            serde_json::from_value(self.used).map_err(|e| OxError::Runtime {
                message: format!("decode provenance_records.used failed: {e}"),
            })?;
        let derived_from: Vec<EntityRef> =
            serde_json::from_value(self.derived_from).map_err(|e| OxError::Runtime {
                message: format!("decode provenance_records.derived_from failed: {e}"),
            })?;
        let was_informed_by: Vec<ProvenanceId> = self
            .was_informed_by
            .into_iter()
            .map(|u| ProvenanceId::new(u.to_string()))
            .collect();
        Ok(ProvenanceDef {
            id: ProvenanceId::new(self.id.to_string()),
            subject,
            activity,
            agent,
            at_time: self.at_time,
            used,
            derived_from,
            was_informed_by,
            plan,
            ontology_valid_at: self.ontology_valid_at,
            data_valid_at: self.data_valid_at,
        })
    }
}

#[async_trait]
impl ProvenanceStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(
        activity_kind = %activity_kind_tag(&capture.activity),
    ))]
    async fn record_activity(
        &self,
        capture: ProvenanceCapture,
        subject: EntityRef,
    ) -> OxResult<ProvenanceId> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let id = Uuid::now_v7();
        let subject_json = serde_json::to_value(&subject).map_err(|e| OxError::Runtime {
            message: format!("encode ProvenanceCapture.subject failed: {e}"),
        })?;
        let activity_json =
            serde_json::to_value(&capture.activity).map_err(|e| OxError::Runtime {
                message: format!("encode ProvenanceCapture.activity failed: {e}"),
            })?;
        let agent_json = serde_json::to_value(&capture.agent).map_err(|e| OxError::Runtime {
            message: format!("encode ProvenanceCapture.agent failed: {e}"),
        })?;
        let plan_json = match &capture.plan {
            Some(plan) => Some(serde_json::to_value(plan).map_err(|e| OxError::Runtime {
                message: format!("encode ProvenanceCapture.plan failed: {e}"),
            })?),
            None => None,
        };
        let used_json = serde_json::to_value(&capture.used).map_err(|e| OxError::Runtime {
            message: format!("encode ProvenanceCapture.used failed: {e}"),
        })?;
        let derived_from_json =
            serde_json::to_value(&capture.derived_from).map_err(|e| OxError::Runtime {
                message: format!("encode ProvenanceCapture.derived_from failed: {e}"),
            })?;
        let was_informed_by: Vec<Uuid> = capture
            .was_informed_by
            .iter()
            .map(|p| {
                Uuid::parse_str(p.as_str()).map_err(|e| OxError::Validation {
                    field: "was_informed_by".into(),
                    message: format!("ProvenanceId `{}` is not a UUID: {e}", p.as_str()),
                })
            })
            .collect::<OxResult<_>>()?;

        sqlx::query(
            "INSERT INTO provenance_records
                (id, workspace_id, subject, activity, agent, plan,
                 used, derived_from, was_informed_by,
                 at_time, ontology_valid_at, data_valid_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), $10, $11)",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(&subject_json)
        .bind(&activity_json)
        .bind(&agent_json)
        .bind(plan_json.as_ref())
        .bind(&used_json)
        .bind(&derived_from_json)
        .bind(&was_informed_by)
        .bind(capture.ontology_valid_at)
        .bind(capture.data_valid_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(ProvenanceId::new(id.to_string()))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(provenance_id = %id))]
    async fn get_provenance_record(&self, id: Uuid) -> OxResult<Option<ProvenanceDef>> {
        super::require_workspace_context()?;
        let row: Option<ProvenanceRow> = sqlx::query_as(
            "SELECT id, subject, activity, agent, plan, used, derived_from,
                    was_informed_by, at_time, ontology_valid_at, data_valid_at
             FROM provenance_records WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(ProvenanceRow::into_def).transpose()
    }
}

/// Render the activity kind discriminator as a static string for
/// the tracing span. `serde_json::to_value` on a known-shape enum
/// cannot fail in practice; the fallback `"<unknown>"` is defensive
/// against a future variant the matcher hasn't been taught.
fn activity_kind_tag(kind: &ProvenanceActivityKind) -> &'static str {
    match kind {
        ProvenanceActivityKind::SourceScan { .. } => "source_scan",
        ProvenanceActivityKind::FunctionEval { .. } => "function_eval",
        ProvenanceActivityKind::RuleValidate { .. } => "rule_validate",
        ProvenanceActivityKind::ActionExecute { .. } => "action_execute",
        ProvenanceActivityKind::OntologyEdit { .. } => "ontology_edit",
        ProvenanceActivityKind::DraftProposal { .. } => "draft_proposal",
        ProvenanceActivityKind::CacheRefresh { .. } => "cache_refresh",
        ProvenanceActivityKind::Enrichment { .. } => "enrichment",
        ProvenanceActivityKind::Import { .. } => "import",
        ProvenanceActivityKind::Export { .. } => "export",
    }
}

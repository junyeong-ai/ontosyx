//! ValueSet orchestration endpoints.
//!
//! The first slice is the `propose` endpoint — given a snapshot of a
//! `SourceSchema` + `SourceProfile`, walk every column and return a
//! `ValueSetInferenceReport` describing columns that should likely
//! become bounded enums. The endpoint is pure and read-only; the
//! caller decides which proposals (if any) to promote into real
//! `CodeSystem` / `ValueSet` / `PropertyDef.value_set_id` bindings
//! via the existing `/edits` surface.

use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_ontology::{
    ValueSetInferencePolicy, ValueSetInferenceReport, ValueSetProposal, ValueSetRejection,
    ValueSetSkip, propose_value_sets,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;

// ---------------------------------------------------------------------------
// POST /api/ontologies/{id}/value-sets/propose
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProposeValueSetsRequest {
    /// Source schema snapshot — usually the `project.source_schema`
    /// column of a design project, but any snapshot produced by the
    /// introspection kernel is acceptable. Wire-shape is defined by
    /// `ox_core::source_schema::SourceSchema`.
    #[schema(value_type = Object)]
    pub schema: SourceSchema,
    /// Profile snapshot (row counts + column stats). Wire-shape is
    /// defined by `ox_core::source_schema::SourceProfile`.
    #[schema(value_type = Object)]
    pub profile: SourceProfile,
    /// Optional policy knobs. Defaults are tuned for high-precision
    /// enum detection.
    #[serde(default)]
    pub policy: Option<ProposePolicyBody>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProposePolicyBody {
    pub distinct_threshold: Option<usize>,
    pub null_ratio_max: Option<f32>,
    pub min_sample_rows: Option<u64>,
    pub min_distinct_count: Option<usize>,
    pub require_full_sample_coverage: Option<bool>,
}

impl ProposePolicyBody {
    fn materialise(self) -> ValueSetInferencePolicy {
        let base = ValueSetInferencePolicy::default();
        ValueSetInferencePolicy {
            distinct_threshold: self.distinct_threshold.unwrap_or(base.distinct_threshold),
            null_ratio_max: self.null_ratio_max.unwrap_or(base.null_ratio_max),
            min_sample_rows: self.min_sample_rows.unwrap_or(base.min_sample_rows),
            min_distinct_count: self.min_distinct_count.unwrap_or(base.min_distinct_count),
            require_full_sample_coverage: self
                .require_full_sample_coverage
                .unwrap_or(base.require_full_sample_coverage),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProposeValueSetsResponse {
    pub ontology_id: Uuid,
    pub proposals: Vec<ProposalBody>,
    pub skipped: Vec<SkipBody>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProposalBody {
    pub relation: String,
    pub column: String,
    pub code_system_id: String,
    pub value_set_id: String,
    pub suggested_codes: Vec<String>,
    pub confidence: f32,
    pub evidence: EvidenceBody,
    /// The full `CodeSystemDef` / `ValueSetDef` JSON — kept flat so
    /// the admin UI can post them verbatim to `/edits` without
    /// re-deriving ids.
    pub code_system_json: serde_json::Value,
    pub value_set_json: serde_json::Value,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvidenceBody {
    pub row_count: u64,
    pub distinct_count: u64,
    pub null_count: u64,
    pub null_ratio: f32,
    pub observed_codes: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SkipBody {
    pub relation: String,
    pub column: String,
    pub reason: String,
}

#[utoipa::path(
    post,
    path = "/api/ontology/value-sets/propose",
    request_body = ProposeValueSetsRequest,
    responses(
        (status = 200, description = "Inference report", body = ProposeValueSetsResponse),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn propose_ontology_value_sets(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    _principal: Principal,
    Json(req): Json<ProposeValueSetsRequest>,
) -> Result<Json<ApiResponse<ProposeValueSetsResponse>>, AppError> {
    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let policy = req
        .policy
        .map(ProposePolicyBody::materialise)
        .unwrap_or_default();
    let report = propose_value_sets(&req.schema, &req.profile, policy);
    Ok(ApiResponse::of(shape_response(identity.id, report)))
}

fn shape_response(ontology_id: Uuid, report: ValueSetInferenceReport) -> ProposeValueSetsResponse {
    let proposals = report.proposals.into_iter().map(shape_proposal).collect();
    let skipped = report.skipped.into_iter().map(shape_skip).collect();
    ProposeValueSetsResponse {
        ontology_id,
        proposals,
        skipped,
    }
}

fn shape_proposal(p: ValueSetProposal) -> ProposalBody {
    let ValueSetProposal {
        column_ref,
        code_system,
        value_set,
        evidence,
        confidence,
    } = p;
    let code_system_json = serde_json::to_value(&code_system).unwrap_or(serde_json::Value::Null);
    let value_set_json = serde_json::to_value(&value_set).unwrap_or(serde_json::Value::Null);
    ProposalBody {
        relation: column_ref.relation,
        column: column_ref.column,
        code_system_id: code_system.id.to_string(),
        value_set_id: value_set.id.to_string(),
        suggested_codes: evidence.observed_codes.clone(),
        confidence,
        evidence: EvidenceBody {
            row_count: evidence.row_count,
            distinct_count: evidence.distinct_count,
            null_count: evidence.null_count,
            null_ratio: evidence.null_ratio,
            observed_codes: evidence.observed_codes,
        },
        code_system_json,
        value_set_json,
    }
}

fn shape_skip(skip: ValueSetSkip) -> SkipBody {
    SkipBody {
        relation: skip.column_ref.relation,
        column: skip.column_ref.column,
        reason: reason_label(&skip.reason),
    }
}

fn reason_label(reason: &ValueSetRejection) -> String {
    match reason {
        ValueSetRejection::TooManyDistinct { distinct } => {
            format!("too_many_distinct ({distinct})")
        }
        ValueSetRejection::TooFewDistinct { distinct } => {
            format!("too_few_distinct ({distinct})")
        }
        ValueSetRejection::TooSparse { row_count } => format!("too_sparse ({row_count} rows)"),
        ValueSetRejection::NullRatioTooHigh { ratio_millis } => {
            format!("null_ratio_too_high ({:.1}%)", *ratio_millis as f32 / 10.0)
        }
        ValueSetRejection::SampleValuesMissing => "sample_values_missing".into(),
        ValueSetRejection::SampleCoverageIncomplete { distinct, sampled } => {
            format!("sample_coverage_incomplete ({sampled}/{distinct})")
        }
    }
}

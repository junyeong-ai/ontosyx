//! NotationPattern orchestration endpoints.
//!
//! Mirrors the `value_sets::propose` surface: given a `SourceSchema` +
//! `SourceProfile` snapshot, walk every column and return a
//! `NotationInferenceReport` describing columns whose sample values
//! consensus-match a structured shape (`SPRING_26_001`,
//! `INV-2025-04231`). The endpoint is pure and read-only; the caller
//! decides which proposals to promote into real `NotationPatternDef`
//! bindings via the existing `/edits` surface.

use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_ontology::{
    NotationInferencePolicy, NotationInferenceRejection, NotationInferenceReport,
    NotationPatternDef, NotationProposal, NotationSkip, propose_notation_patterns,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;

// ---------------------------------------------------------------------------
// POST /api/ontology/notation-patterns/propose
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProposeNotationPatternsRequest {
    /// Source schema snapshot — usually the `project.source_schema`
    /// column of a design project. Wire-shape is defined by
    /// `ox_core::source_schema::SourceSchema`.
    pub schema: SourceSchema,
    /// Profile snapshot (row counts + column stats). Wire-shape is
    /// defined by `ox_core::source_schema::SourceProfile`.
    pub profile: SourceProfile,
    /// Optional policy knobs. Defaults are tuned for high-precision
    /// pattern detection (`min_samples = 3`, full agreement required).
    #[serde(default)]
    pub policy: Option<NotationPolicyBody>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct NotationPolicyBody {
    pub min_samples: Option<usize>,
    pub require_full_agreement: Option<bool>,
}

impl NotationPolicyBody {
    fn materialise(self) -> NotationInferencePolicy {
        let base = NotationInferencePolicy::default();
        NotationInferencePolicy {
            min_samples: self.min_samples.unwrap_or(base.min_samples),
            require_full_agreement: self
                .require_full_agreement
                .unwrap_or(base.require_full_agreement),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProposeNotationPatternsResponse {
    pub ontology_id: Uuid,
    pub proposals: Vec<NotationProposalBody>,
    pub skipped: Vec<NotationSkipBody>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NotationProposalBody {
    pub relation: String,
    pub column: String,
    pub pattern_id: String,
    pub template: String,
    pub separator: String,
    pub examples: Vec<String>,
    pub confidence: f64,
    /// Full `NotationPatternDef` JSON — kept flat so the admin UI can
    /// post it verbatim to `/edits` without re-deriving the id.
    pub pattern_json: NotationPatternDef,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NotationSkipBody {
    pub relation: String,
    pub column: String,
    pub reason: String,
}

#[utoipa::path(
    post,
    path = "/api/ontology/notation-patterns/propose",
    request_body = ProposeNotationPatternsRequest,
    responses(
        (status = 200, description = "Inference report", body = ProposeNotationPatternsResponse),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn propose_ontology_notation_patterns(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    _principal: Principal,
    Json(req): Json<ProposeNotationPatternsRequest>,
) -> Result<Json<ApiResponse<ProposeNotationPatternsResponse>>, AppError> {
    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let policy = req
        .policy
        .map(NotationPolicyBody::materialise)
        .unwrap_or_default();
    let report = propose_notation_patterns(&req.schema, &req.profile, policy);
    Ok(ApiResponse::of(shape_response(identity.id, report)))
}

fn shape_response(
    ontology_id: Uuid,
    report: NotationInferenceReport,
) -> ProposeNotationPatternsResponse {
    let proposals = report.proposals.into_iter().map(shape_proposal).collect();
    let skipped = report.skipped.into_iter().map(shape_skip).collect();
    ProposeNotationPatternsResponse {
        ontology_id,
        proposals,
        skipped,
    }
}

fn shape_proposal(p: NotationProposal) -> NotationProposalBody {
    let NotationProposal {
        column_ref,
        pattern,
        examples,
        confidence,
    } = p;
    NotationProposalBody {
        relation: column_ref.relation,
        column: column_ref.column,
        pattern_id: pattern.id.to_string(),
        template: pattern.template.clone(),
        separator: pattern.separator.clone(),
        examples,
        confidence,
        pattern_json: pattern,
    }
}

fn shape_skip(skip: NotationSkip) -> NotationSkipBody {
    NotationSkipBody {
        relation: skip.column_ref.relation,
        column: skip.column_ref.column,
        reason: reason_label(&skip.reason),
    }
}

fn reason_label(reason: &NotationInferenceRejection) -> String {
    match reason {
        NotationInferenceRejection::InsufficientSamples {
            available,
            required,
        } => {
            format!("insufficient_samples ({available}/{required})")
        }
        NotationInferenceRejection::TokenCountMismatch { observed_counts } => {
            let counts: Vec<String> = observed_counts.iter().map(|n| n.to_string()).collect();
            format!("token_count_mismatch ({})", counts.join(","))
        }
        NotationInferenceRejection::ClassDisagreement { position, observed } => {
            format!(
                "class_disagreement at pos {position} ({})",
                observed.join("|")
            )
        }
        NotationInferenceRejection::SeparatorDisagreement { observed } => {
            format!("separator_disagreement ({})", observed.join("|"))
        }
        NotationInferenceRejection::Unstructured => "unstructured".into(),
    }
}

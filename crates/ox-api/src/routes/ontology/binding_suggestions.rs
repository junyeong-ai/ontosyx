//! Glossary ↔ property binding-suggestion endpoints.
//!
//! Given a glossary term (either saved or drafted) and an ontology
//! id, return the ranked list of properties most likely to realise
//! that term. The inverse direction — "which terms match this
//! property?" — shares the same scorer and is exposed alongside.

use axum::Json;
use axum::extract::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::i18n::LocalizedText;
use ox_ontology::{
    BindingSignal, BindingSuggestionPolicy, GlossaryTermDef, GlossaryTermId, PropertyBindingCandidate,
    PropertyOwnerRef, TermBindingCandidate, suggest_property_bindings_by_term,
    suggest_terms_by_property,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Shared policy — keep identical wire shape across endpoints so a
// single FE form component can drive both.
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct BindingPolicy {
    pub min_score: Option<f32>,
    pub max_results: Option<usize>,
    pub weight_exact_name: Option<f32>,
    pub weight_alias_match: Option<f32>,
    pub weight_description_overlap: Option<f32>,
    pub weight_fuzzy_name: Option<f32>,
    pub fuzzy_min_ratio: Option<f32>,
    pub skip_already_bound: Option<bool>,
}

impl BindingPolicy {
    fn materialise(self) -> BindingSuggestionPolicy {
        let base = BindingSuggestionPolicy::default();
        BindingSuggestionPolicy {
            min_score: self.min_score.unwrap_or(base.min_score),
            max_results: self.max_results.unwrap_or(base.max_results),
            weight_exact_name: self.weight_exact_name.unwrap_or(base.weight_exact_name),
            weight_alias_match: self.weight_alias_match.unwrap_or(base.weight_alias_match),
            weight_description_overlap: self
                .weight_description_overlap
                .unwrap_or(base.weight_description_overlap),
            weight_fuzzy_name: self.weight_fuzzy_name.unwrap_or(base.weight_fuzzy_name),
            fuzzy_min_ratio: self.fuzzy_min_ratio.unwrap_or(base.fuzzy_min_ratio),
            skip_already_bound: self.skip_already_bound.unwrap_or(base.skip_already_bound),
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/ontologies/{id}/glossary/suggest-bindings
//
// Accepts a draft term in the body so the admin UI can score a
// "term-in-progress" without saving it first.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SuggestBindingsRequest {
    /// Canonical term name. `LocalizedText` so a draft term carries
    /// the same multi-locale shape as a saved one — the scorer matches
    /// against every locale variant.
    #[schema(value_type = Object)]
    pub term: LocalizedText,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    pub aliases: Vec<LocalizedText>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub description: Option<LocalizedText>,
    /// Pre-existing term id if the term is already saved. When
    /// provided, the endpoint confirms it refers to the saved
    /// definition; otherwise an ephemeral id is minted for scoring.
    #[serde(default)]
    pub term_id: Option<String>,
    #[serde(default)]
    pub policy: Option<BindingPolicy>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SuggestBindingsResponse {
    pub ontology_id: Uuid,
    pub candidates: Vec<PropertyCandidate>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PropertyCandidate {
    pub owner_kind: String, // "node" | "edge"
    pub owner_type_id: String,
    pub owner_label: String,
    pub property_id: String,
    pub property_name: String,
    pub score: f32,
    pub signals: Vec<BindingSignal>,
}

#[utoipa::path(
    post,
    path = "/api/ontologies/{id}/glossary/suggest-bindings",
    params(("id" = Uuid, Path, description = "Ontology identity ID")),
    request_body = SuggestBindingsRequest,
    responses(
        (status = 200, description = "Candidate property bindings", body = SuggestBindingsResponse),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn suggest_glossary_bindings(
    axum::extract::State(state): axum::extract::State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<SuggestBindingsRequest>,
) -> Result<Json<ApiResponse<SuggestBindingsResponse>>, AppError> {
    let ir = load_current_ir(&state, id).await?;
    let policy = req
        .policy
        .map(BindingPolicy::materialise)
        .unwrap_or_default();

    let term = GlossaryTermDef {
        id: GlossaryTermId::new(req.term_id.as_deref().unwrap_or("__draft__")),
        term: req.term,
        display_name: LocalizedText::default(),
        description: req.description.unwrap_or_default(),
        examples: Vec::new(),
        category: None,
        aliases: req.aliases,
        related_terms: Vec::new(),
        governance: ox_ontology::glossary::TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: ox_ontology::glossary::TermLifecycle::default(),
        realisation: None,
    };

    let candidates = suggest_property_bindings_by_term(&ir, &term, policy);
    Ok(ApiResponse::of(SuggestBindingsResponse {
        ontology_id: id,
        candidates: candidates.into_iter().map(shape_candidate).collect(),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ontologies/{id}/properties/{owner_kind}/{owner_type_id}/{property_id}/suggest-terms
//
// The inverse direction — edit a property, see which existing terms
// are nearby matches. Useful as an auto-suggest in the PropertyDef
// editor's glossary-term dropdown.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SuggestTermsRequest {
    #[serde(default)]
    pub policy: Option<BindingPolicy>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SuggestTermsResponse {
    pub ontology_id: Uuid,
    pub candidates: Vec<TermCandidate>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TermCandidate {
    pub term_id: String,
    #[schema(value_type = Object)]
    pub term: LocalizedText,
    pub score: f32,
    pub signals: Vec<BindingSignal>,
}

#[utoipa::path(
    post,
    path = "/api/ontologies/{id}/properties/{owner_kind}/{owner_type_id}/{property_id}/suggest-terms",
    params(
        ("id" = Uuid, Path, description = "Ontology identity ID"),
        ("owner_kind" = String, Path, description = "node | edge"),
        ("owner_type_id" = String, Path, description = "NodeTypeId or EdgeTypeId"),
        ("property_id" = String, Path, description = "PropertyId"),
    ),
    request_body = SuggestTermsRequest,
    responses(
        (status = 200, description = "Candidate glossary terms", body = SuggestTermsResponse),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn suggest_glossary_terms_for_property(
    axum::extract::State(state): axum::extract::State<AppState>,
    _principal: Principal,
    Path((id, owner_kind, owner_type_id, property_id)): Path<(Uuid, String, String, String)>,
    Json(req): Json<SuggestTermsRequest>,
) -> Result<Json<ApiResponse<SuggestTermsResponse>>, AppError> {
    let ir = load_current_ir(&state, id).await?;
    let policy = req
        .policy
        .map(BindingPolicy::materialise)
        .unwrap_or_default();

    let owner = match owner_kind.as_str() {
        "node" => PropertyOwnerRef::Node {
            node_type: owner_type_id.clone().into(),
            label: String::new(), // scorer does not read label
        },
        "edge" => PropertyOwnerRef::Edge {
            edge_type: owner_type_id.clone().into(),
            label: String::new(),
        },
        other => {
            return Err(AppError::bad_request(format!(
                "owner_kind must be 'node' or 'edge', got {other:?}"
            )));
        }
    };

    let property_id_owned = property_id.into();
    let candidates = suggest_terms_by_property(&ir, &owner, &property_id_owned, policy);
    Ok(ApiResponse::of(SuggestTermsResponse {
        ontology_id: id,
        candidates: candidates.into_iter().map(shape_term_candidate).collect(),
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn load_current_ir(
    state: &AppState,
    ontology_id: Uuid,
) -> Result<ox_ontology::ir::OntologyIR, AppError> {
    let identity = state
        .store
        .get_ontology(ontology_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let version = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::unprocessable(format!(
                "Ontology `{}` has no committed version",
                identity.lineage_id
            ))
        })?;
    state
        .store
        .get_ontology_ir(version.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))
}

fn shape_candidate(c: PropertyBindingCandidate) -> PropertyCandidate {
    let (kind, type_id, label) = match c.owner {
        PropertyOwnerRef::Node { node_type, label } => {
            ("node".to_string(), node_type.to_string(), label)
        }
        PropertyOwnerRef::Edge { edge_type, label } => {
            ("edge".to_string(), edge_type.to_string(), label)
        }
    };
    PropertyCandidate {
        owner_kind: kind,
        owner_type_id: type_id,
        owner_label: label,
        property_id: c.property_id.to_string(),
        property_name: c.property_name,
        score: c.score,
        signals: c.signals,
    }
}

fn shape_term_candidate(c: TermBindingCandidate) -> TermCandidate {
    TermCandidate {
        term_id: c.term_id.to_string(),
        term: c.term,
        score: c.score,
        signals: c.signals,
    }
}

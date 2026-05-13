//! `/api/ontology/communities` — Microsoft GraphRAG community-
//! summary CRUD surface.
//!
//! Workspace × ontology = 1:1 invariant means the URL doesn't
//! carry an ontology id — every endpoint resolves the
//! workspace's canonical ontology + current version
//! automatically. Operators / detection cron post summaries
//! against the latest version; the agent's GraphRAG retrieval
//! reads them via `OntologyNavigationStore + CommunitySummaryStore`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::community::CommunitySummary;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

const MAX_TOP_K: u32 = 100;
const MAX_LEVEL: u32 = i16::MAX as u32;
const MAX_MEMBERS: usize = 10_000;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpsertCommunitySummaryRequest {
    #[schema(min_length = 1)]
    pub community_id: String,
    #[serde(default)]
    #[schema(minimum = 0, maximum = 32767)]
    pub level: u32,
    #[serde(default)]
    #[schema(max_items = 10000)]
    pub member_entity_kinds: Vec<String>,
    #[serde(default)]
    #[schema(max_items = 10000)]
    pub member_logical_ids: Vec<String>,
    #[schema(min_length = 1)]
    pub title: String,
    #[schema(min_length = 1)]
    pub summary: String,
}

impl UpsertCommunitySummaryRequest {
    fn validate(&self) -> Result<(), AppError> {
        if self.community_id.trim().is_empty() {
            return Err(AppError::validation(
                "community_id",
                "community_id must not be empty",
            ));
        }
        if self.title.trim().is_empty() {
            return Err(AppError::validation("title", "title must not be empty"));
        }
        if self.summary.trim().is_empty() {
            return Err(AppError::validation("summary", "summary must not be empty"));
        }
        if self.level > MAX_LEVEL {
            return Err(AppError::validation(
                "level",
                &format!("level must be between 0 and {MAX_LEVEL}"),
            ));
        }
        if self.member_entity_kinds.len() != self.member_logical_ids.len() {
            return Err(AppError::validation(
                "member_entity_kinds",
                "member_entity_kinds and member_logical_ids must have the same length",
            ));
        }
        if self
            .member_entity_kinds
            .iter()
            .any(|kind| kind.trim().is_empty())
        {
            return Err(AppError::validation(
                "member_entity_kinds",
                "member_entity_kinds must not contain empty values",
            ));
        }
        if self
            .member_logical_ids
            .iter()
            .any(|logical_id| logical_id.trim().is_empty())
        {
            return Err(AppError::validation(
                "member_logical_ids",
                "member_logical_ids must not contain empty values",
            ));
        }
        if self.member_entity_kinds.len() > MAX_MEMBERS {
            return Err(AppError::validation(
                "member_entity_kinds",
                "community summaries support at most 10000 members",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CommunitySummaryDto {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub ontology_version_id: Uuid,
    #[schema(min_length = 1)]
    pub community_id: String,
    #[schema(minimum = 0, maximum = 32767)]
    pub level: u32,
    #[schema(max_items = 10000)]
    pub member_entity_kinds: Vec<String>,
    #[schema(max_items = 10000)]
    pub member_logical_ids: Vec<String>,
    #[schema(min_length = 1)]
    pub title: String,
    #[schema(min_length = 1)]
    pub summary: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl From<CommunitySummary> for CommunitySummaryDto {
    fn from(summary: CommunitySummary) -> Self {
        Self {
            id: summary.id,
            workspace_id: summary.workspace_id,
            ontology_version_id: summary.ontology_version_id,
            community_id: summary.community_id,
            level: summary.level,
            member_entity_kinds: summary.member_entity_kinds,
            member_logical_ids: summary.member_logical_ids,
            title: summary.title,
            summary: summary.summary,
            generated_at: summary.generated_at,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CommunitySummaryResponse {
    pub summary: CommunitySummaryDto,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListCommunitySummariesResponse {
    pub items: Vec<CommunitySummaryDto>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SearchCommunitySummariesQuery {
    #[schema(min_length = 1)]
    pub q: String,
    #[serde(default = "default_top_k")]
    #[schema(minimum = 1, maximum = 100)]
    pub top_k: u32,
}

impl SearchCommunitySummariesQuery {
    fn validate(&self) -> Result<(), AppError> {
        if self.q.trim().is_empty() {
            return Err(AppError::validation("q", "q must not be empty"));
        }
        if self.top_k == 0 || self.top_k > MAX_TOP_K {
            return Err(AppError::validation(
                "top_k",
                "top_k must be between 1 and 100",
            ));
        }
        Ok(())
    }
}

fn default_top_k() -> u32 {
    10
}

fn into_list_response(items: Vec<CommunitySummary>) -> ListCommunitySummariesResponse {
    ListCommunitySummariesResponse {
        items: items.into_iter().map(CommunitySummaryDto::from).collect(),
    }
}

/// Resolve the workspace's canonical version id. Fails with a
/// typed 400 when the workspace has no committed canonical yet
/// — community summaries are version-keyed, so a draft-only
/// workspace can't author them. `commit a draft first` is the
/// operator-actionable next step the FE renders.
async fn resolve_canonical_version_id(state: &AppState) -> Result<Uuid, AppError> {
    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::query_ir_invalid(
                "workspace has no canonical ontology yet — \
                 commit a draft before authoring community \
                 summaries"
                    .to_string(),
            )
        })?;
    let version = state
        .store
        .find_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::query_ir_invalid("workspace ontology has no committed version".to_string())
        })?;
    Ok(version.id)
}

/// `GET /api/ontology/communities` — list every community
/// summary attached to the workspace's canonical version.
/// Sorted `(level ASC, community_id ASC)` for deterministic
/// hierarchical FE rendering.
#[utoipa::path(
    get,
    path = "/api/ontology/communities",
    responses(
        (status = 200, description = "Community summaries", body = ListCommunitySummariesResponse),
        (status = 400, description = "Workspace has no canonical ontology yet",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn list_community_summaries(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
) -> Result<Json<ApiResponse<ListCommunitySummariesResponse>>, AppError> {
    let version_id = resolve_canonical_version_id(&state).await?;
    let items = state
        .store
        .list_community_summaries_for_version(version_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(into_list_response(items)))
}

/// `POST /api/ontology/communities` — upsert on
/// `(ontology_version_id, community_id)`. Re-summarizing under
/// the same id replaces in place; lineage / reverse-index
/// queries against the id continue to resolve.
#[utoipa::path(
    post,
    path = "/api/ontology/communities",
    request_body = UpsertCommunitySummaryRequest,
    responses(
        (status = 200, description = "Community summary upserted", body = CommunitySummaryResponse),
        (status = 400, description = "Workspace has no canonical ontology yet",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn upsert_community_summary(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<UpsertCommunitySummaryRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CommunitySummaryResponse>>), AppError> {
    principal.require_admin()?;
    req.validate()?;
    let version_id = resolve_canonical_version_id(&state).await?;
    let member_entity_kinds: Vec<String> = req
        .member_entity_kinds
        .into_iter()
        .map(|kind| kind.trim().to_string())
        .collect();
    let member_logical_ids: Vec<String> = req
        .member_logical_ids
        .into_iter()
        .map(|logical_id| logical_id.trim().to_string())
        .collect();
    let member_fingerprint =
        CommunitySummary::compute_member_fingerprint(&member_entity_kinds, &member_logical_ids);
    let summary = CommunitySummary {
        id: Uuid::now_v7(),
        workspace_id: ws.workspace_id,
        ontology_version_id: version_id,
        community_id: req.community_id.trim().to_string(),
        level: req.level,
        member_entity_kinds,
        member_logical_ids,
        member_fingerprint,
        title: req.title.trim().to_string(),
        summary: req.summary.trim().to_string(),
        tokenized_text: String::new(),
        tokenizer_dict_fingerprint: String::new(),
        embedding: None,
        generated_at: chrono::Utc::now(),
    };
    let saved = state
        .store
        .upsert_community_summary(&summary)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::OK,
        ApiResponse::of(CommunitySummaryResponse {
            summary: CommunitySummaryDto::from(saved),
        }),
    ))
}

/// `GET /api/ontology/communities/search?q=…&top_k=10` —
/// trigram-blended search over title + summary. Drives the
/// future FE preview pane that lets operators sanity-check
/// retrieval before committing summaries.
#[utoipa::path(
    get,
    path = "/api/ontology/communities/search",
    params(
        ("q" = String, Query, description = "Search query"),
        ("top_k" = Option<u32>, Query, description = "Max results (default 10)"),
    ),
    responses(
        (status = 200, description = "Matching community summaries", body = ListCommunitySummariesResponse),
        (status = 400, description = "Workspace has no canonical ontology yet",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn search_community_summaries(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
    Query(req): Query<SearchCommunitySummariesQuery>,
) -> Result<Json<ApiResponse<ListCommunitySummariesResponse>>, AppError> {
    req.validate()?;
    let version_id = resolve_canonical_version_id(&state).await?;
    let question_raw = req.q.trim();
    let tokens =
        crate::tokenizer_publish::tokenize_for_workspace(&state, ws.workspace_id, question_raw)
            .await;
    let items = state
        .store
        .search_community_summaries(
            version_id,
            question_raw,
            &tokens.tokenized_text,
            None,
            req.top_k,
        )
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(into_list_response(items)))
}

/// `DELETE /api/ontology/communities/{id}` — remove a single
/// summary by primary key. Returns 404 when the row doesn't
/// exist (or RLS hides it cross-workspace).
#[utoipa::path(
    delete,
    path = "/api/ontology/communities/{id}",
    params(("id" = Uuid, Path, description = "Community summary id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn delete_community_summary(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    let deleted = state
        .store
        .delete_community_summary(id)
        .await
        .map_err(AppError::from)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("CommunitySummary"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_upsert_request() -> UpsertCommunitySummaryRequest {
        UpsertCommunitySummaryRequest {
            community_id: "leiden:0:7".into(),
            level: 0,
            member_entity_kinds: vec!["NodeType".into(), "Concept".into()],
            member_logical_ids: vec!["nt_customer".into(), "c_vip".into()],
            title: "Premium customer cluster".into(),
            summary: "Customers with VIP tier and high-value order behavior.".into(),
        }
    }

    #[test]
    fn upsert_request_rejects_empty_identity_fields() {
        let mut req = valid_upsert_request();
        req.community_id = "  ".into();
        assert!(req.validate().is_err());

        let mut req = valid_upsert_request();
        req.title = "  ".into();
        assert!(req.validate().is_err());

        let mut req = valid_upsert_request();
        req.summary = "  ".into();
        assert!(req.validate().is_err());
    }

    #[test]
    fn upsert_request_requires_parallel_member_arrays() {
        let mut req = valid_upsert_request();
        req.member_logical_ids.pop();

        assert!(req.validate().is_err());
    }

    #[test]
    fn upsert_request_rejects_out_of_range_level() {
        let mut req = valid_upsert_request();
        req.level = MAX_LEVEL + 1;

        assert!(req.validate().is_err());
    }

    #[test]
    fn upsert_request_rejects_blank_member_values() {
        let mut req = valid_upsert_request();
        req.member_entity_kinds[0] = " ".into();
        assert!(req.validate().is_err());

        let mut req = valid_upsert_request();
        req.member_logical_ids[0] = " ".into();
        assert!(req.validate().is_err());
    }

    #[test]
    fn upsert_request_accepts_valid_payload() {
        assert!(valid_upsert_request().validate().is_ok());
    }

    #[test]
    fn search_query_rejects_empty_query_and_out_of_range_top_k() {
        assert!(
            SearchCommunitySummariesQuery {
                q: " ".into(),
                top_k: 10,
            }
            .validate()
            .is_err()
        );
        assert!(
            SearchCommunitySummariesQuery {
                q: "customer".into(),
                top_k: 0,
            }
            .validate()
            .is_err()
        );
        assert!(
            SearchCommunitySummariesQuery {
                q: "customer".into(),
                top_k: MAX_TOP_K + 1,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn search_query_accepts_bounds() {
        assert!(
            SearchCommunitySummariesQuery {
                q: " customer ".into(),
                top_k: MAX_TOP_K,
            }
            .validate()
            .is_ok()
        );
    }
}

//! `GET /api/ontologies/type-candidates?logical_id=...&kind=node|edge`
//!
//! Given the stable logical id of a node or edge type, return every
//! ontology in the current workspace whose latest committed version
//! contains a matching entry. Used by the Phase 4.7 stale-concept
//! approval flow: after an admin approves a proposal, the UI looks
//! up which ontologies own the type so it can post a
//! `DeprecateNodeType` / `DeprecateEdgeType` edit op against the
//! right ontology.
//!
//! Ambiguity is intentional output, not an error — a forked ontology
//! may carry the same logical id in multiple lineages. The UI picks
//! one (or asks the user) and calls `/ontologies/{id}/edits` itself.
//!
//! Lineage-scoped because `list_ontologies` is RLS-scoped to the
//! caller's workspace.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TypeCandidatesParams {
    /// Stable id string. For types that were created via auto-uuid,
    /// this is the UUID rendered as a string; for authored ids, the
    /// author-assigned value. Match is exact.
    pub logical_id: String,
    /// `"node"` or `"edge"`. Accept the Phase 3 signal form
    /// (`"NodeType"` / `"EdgeType"`) too so the FE can forward
    /// `StaleConceptProposal.type_kind` unchanged.
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeKind {
    Node,
    Edge,
}

impl TypeKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "node" | "NodeType" | "node_type" => Some(Self::Node),
            "edge" | "EdgeType" | "edge_type" => Some(Self::Edge),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TypeCandidate {
    pub ontology_id: Uuid,
    pub ontology_name: String,
    /// Version string on the current snapshot. Parseable as u32 in
    /// practice — surfaced verbatim so the FE can render it as-is
    /// and pass it back as `expected_version` on the edit POST.
    pub current_version: String,
    /// Graph label of the matching type. The admin UI renders this
    /// alongside the ontology name so a human can pick the right
    /// row when multiple ontologies match.
    pub label: String,
    /// `Some(ts)` when the type is already deprecated in this
    /// ontology's current version. The UI dims the row so the
    /// admin doesn't double-deprecate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Enumerate every ontology in the workspace whose current version
/// contains a node/edge type with the given logical id.
///
/// Iterative scan — O(n) ontologies × O(m) types per version. For
/// the expected workspace scale (dozens of ontologies, each with
/// low-hundreds of types) this is well under 100ms. If scan cost
/// becomes a concern, swap to a direct query on
/// `ontology_version_entities` keyed on `(kind, logical_id)`.
pub(crate) async fn list_type_candidates(
    State(state): State<AppState>,
    _principal: Principal,
    Query(params): Query<TypeCandidatesParams>,
) -> Result<Json<ApiResponse<Vec<TypeCandidate>>>, AppError> {
    let kind = TypeKind::parse(&params.kind).ok_or_else(|| {
        AppError::bad_request(format!(
            "kind must be one of \"node\" / \"edge\" (or the NodeType/EdgeType aliases); got \"{}\"",
            params.kind
        ))
    })?;

    // Pull the full workspace. `list_ontologies` is cursor-paginated;
    // the expected scale (≪ 100 ontologies per workspace) fits in
    // one page, and the scan below is cheap enough that paginating
    // client-side would only add latency.
    let pagination = CursorParams {
        limit: 100,
        cursor: None,
    };
    let page = state
        .store
        .list_ontologies(&pagination)
        .await
        .map_err(AppError::from)?;

    let mut out = Vec::new();
    for row in page.items {
        let current = state
            .store
            .get_current_version(row.id)
            .await
            .map_err(AppError::from)?;
        let Some(version) = current else { continue };
        let ir = match state.store.load_version(version.id).await {
            Ok(ir) => ir,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    ontology_id = %row.id,
                    version_id = %version.id,
                    "type_candidates: load_version failed; skipping",
                );
                continue;
            }
        };

        match kind {
            TypeKind::Node => {
                for node in ir.node_types() {
                    if node.id.as_str() == params.logical_id {
                        out.push(TypeCandidate {
                            ontology_id: row.id,
                            ontology_name: row.name.clone(),
                            current_version: version.version.clone(),
                            label: node.label.as_str().to_string(),
                            deprecated_at: node.deprecated_at,
                        });
                    }
                }
            }
            TypeKind::Edge => {
                for edge in ir.edge_types() {
                    if edge.id.as_str() == params.logical_id {
                        out.push(TypeCandidate {
                            ontology_id: row.id,
                            ontology_name: row.name.clone(),
                            current_version: version.version.clone(),
                            label: edge.label.as_str().to_string(),
                            deprecated_at: edge.deprecated_at,
                        });
                    }
                }
            }
        }
    }

    Ok(ApiResponse::of(out))
}

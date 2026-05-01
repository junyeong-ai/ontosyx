//! `GET /api/ontologies/{id}/axis-items?kind=<kind>` — drill-down
//! companion to `/map-summary`. Given one of the axis kinds the
//! summary surfaces (e.g. `node_types`, `glossary_terms`,
//! `value_sets`), returns the list of matching entries in the
//! ontology's current version so the Complete Map page can render
//! an inline drill-down.
//!
//! Intentionally narrow: returns an `{id, label, description}`
//! triple per row. The label resolver picks the most specific
//! piece of text the entity exposes — `label` for typed topologies,
//! `term` for glossary, `id` verbatim as the fallback. The map
//! page renders them as a plain list; a dedicated editor for each
//! kind is out of scope here.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AxisItemsParams {
    /// One of the kind strings the `/map-summary` endpoint emits —
    /// `"node_types" | "edge_types" | "indexes" | "glossary_terms" |
    /// "interfaces" | "code_systems" | "value_sets" |
    /// "notation_patterns" | "concept_maps" | "value_range_sets" |
    /// "rules" | "actions" | "functions" | "metrics" |
    /// "object_mappings" | "link_mappings" | "provenances" |
    /// "data_qualities" | "enrichments"`.
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct AxisItem {
    /// Stable id string for the entity.
    pub id: String,
    /// Human-readable label. Falls back to `id` when no richer
    /// text is available on the entity.
    pub label: String,
    /// Optional free-form description. May be empty even when the
    /// entity type normally carries one; the FE treats `""` as
    /// "omit the description line".
    pub description: String,
}

#[utoipa::path(
    get,
    path = "/api/ontologies/{id}/axis-items",
    params(
        ("id" = Uuid, Path, description = "Ontology identity id"),
        ("kind" = String, Query, description = "Axis kind string from /map-summary"),
    ),
    responses(
        (status = 200, description = "Axis items list", body = Object),
        (status = 400, description = "Unknown kind string"),
        (status = 404, description = "Ontology not found or has no committed version"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn list_axis_items(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
    Query(params): Query<AxisItemsParams>,
) -> Result<Json<ApiResponse<Vec<AxisItem>>>, AppError> {
    let identity = state
        .store
        .get_ontology(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let current = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ontology has no committed version"))?;
    let ir = state
        .store
        .get_ontology_ir(current.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;

    let items = collect_axis_items(&ir, &params.kind).ok_or_else(|| {
        AppError::bad_request(format!("unknown axis kind: \"{}\"", params.kind))
    })?;

    Ok(ApiResponse::of(items))
}

/// Pure extractor — matches an axis kind string against the IR and
/// returns the triple list. Returning `Option::None` when the kind
/// is unknown gives the caller a clean branch for 400 without
/// threading an error type through the IR walk.
fn collect_axis_items(ir: &ox_ontology::OntologyIR, kind: &str) -> Option<Vec<AxisItem>> {
    use ox_core::i18n::LocalizedText;
    fn lt_to_string(lt: &LocalizedText) -> String {
        // The canonical default is always present (may be empty
        // when the designer left the field blank). Returning the
        // owned string — the caller renders it verbatim and an
        // empty string tells the FE to drop the description line.
        lt.default_str().to_string()
    }
    Some(match kind {
        "node_types" => ir
            .node_types()
            .iter()
            .map(|n| AxisItem {
                id: n.id.as_str().to_string(),
                label: n.label.as_str().to_string(),
                description: lt_to_string(&n.description),
            })
            .collect(),
        "edge_types" => ir
            .edge_types()
            .iter()
            .map(|e| AxisItem {
                id: e.id.as_str().to_string(),
                label: e.label.as_str().to_string(),
                description: lt_to_string(&e.description),
            })
            .collect(),
        "indexes" => ir
            .indexes()
            .iter()
            .map(|i| {
                use ox_ontology::ir::IndexDef;
                let (id, label) = match i {
                    IndexDef::Single { id, .. }
                    | IndexDef::Composite { id, .. }
                    | IndexDef::Vector { id, .. } => (id.clone(), id.clone()),
                    IndexDef::FullText { id, name, .. } => {
                        (id.clone(), name.as_str().to_string())
                    }
                };
                AxisItem {
                    id,
                    label,
                    description: String::new(),
                }
            })
            .collect(),
        "glossary_terms" => ir
            .glossary()
            .iter()
            .map(|g| AxisItem {
                id: g.id.as_str().to_string(),
                label: lt_to_string(&g.term),
                description: lt_to_string(&g.description),
            })
            .collect(),
        "interfaces" => ir
            .interfaces()
            .iter()
            .map(|i| AxisItem {
                id: i.id.as_str().to_string(),
                label: i.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "code_systems" => ir
            .code_systems()
            .iter()
            .map(|c| AxisItem {
                id: c.id.as_str().to_string(),
                label: c.name.clone(),
                description: lt_to_string(&c.description),
            })
            .collect(),
        "value_sets" => ir
            .value_sets()
            .iter()
            .map(|v| AxisItem {
                id: v.id.as_str().to_string(),
                label: v.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "notation_patterns" => ir
            .notation_patterns()
            .iter()
            .map(|n| AxisItem {
                id: n.id.as_str().to_string(),
                label: n.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "concept_maps" => ir
            .concept_maps()
            .iter()
            .map(|c| AxisItem {
                id: c.id.as_str().to_string(),
                label: c.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "value_range_sets" => ir
            .value_range_sets()
            .iter()
            .map(|v| AxisItem {
                id: v.id.as_str().to_string(),
                label: v.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "rules" => ir
            .rules()
            .iter()
            .map(|r| AxisItem {
                id: r.id.as_str().to_string(),
                label: r.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "actions" => ir
            .actions()
            .iter()
            .map(|a| AxisItem {
                id: a.id.as_str().to_string(),
                label: a.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "functions" => ir
            .functions()
            .iter()
            .map(|f| AxisItem {
                id: f.id.as_str().to_string(),
                label: f.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "metrics" => ir
            .metrics()
            .iter()
            .map(|m| AxisItem {
                id: m.id.as_str().to_string(),
                label: m.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "object_mappings" => ir
            .object_mappings()
            .iter()
            .map(|m| AxisItem {
                id: m.id.as_str().to_string(),
                label: m.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "link_mappings" => ir
            .link_mappings()
            .iter()
            .map(|m| AxisItem {
                id: m.id.as_str().to_string(),
                label: m.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "provenances" => ir
            .provenance()
            .iter()
            .map(|p| AxisItem {
                id: p.id.as_str().to_string(),
                label: p.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "data_qualities" => ir
            .data_quality()
            .iter()
            .map(|d| AxisItem {
                id: d.id.as_str().to_string(),
                label: d.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        "enrichments" => ir
            .enrichments()
            .iter()
            .map(|e| AxisItem {
                id: e.id.as_str().to_string(),
                label: e.id.as_str().to_string(),
                description: String::new(),
            })
            .collect(),
        _ => return None,
    })
}

//! `GET /api/ontologies/{id}/cross-refs` — enumerate every pointer
//! field in the current-version IR that crosses from one axis to
//! another (or within the same axis, when the link is semantically
//! interesting — e.g. `NodeType.parent`).
//!
//! Feeds the Phase 4.2 follow-up visualisation on the Complete Map
//! page. The FE groups these edges by `(source_axis, target_axis)`
//! to draw a small 6-node React-Flow-style diagram; individual
//! edges surface in a drill-down list.
//!
//! This handler is intentionally exhaustive within the current
//! pointer surface but does not walk every `Option<XxxId>` in the
//! IR. The tail (Function dependencies / Interface required edges
//! / Metric target scope) can be added as the visualisation grows.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use uuid::Uuid;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

/// Six-axis labels, matching the Complete Map dashboard. Every
/// variant is a public wire contract — the FE filters / groups
/// edges by axis — so variants with no current emitter stay in the
/// enum rather than being removed; `#[allow(dead_code)]` is what
/// keeps that public surface honest without a linter nag.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Topology,
    Vocabulary,
    Registry,
    Strategy,
    Vol,
    Governance,
}

#[derive(Debug, Serialize)]
pub struct CrossRefEdge {
    /// Axis that owns `source_id`. Drives node placement on the
    /// FE grouped layout.
    pub source_axis: Axis,
    /// Entity kind (`"property"`, `"node_type"`, etc.). The FE maps
    /// this to an icon / label style independent of the axis.
    pub source_kind: String,
    /// Stable string id of the source entity. Compound ids like
    /// `"node:Customer/tier"` point at a property inside its
    /// owning node — the FE can split on `"/"` when it needs to
    /// zoom past the node grouping.
    pub source_id: String,
    /// Kind of relation — `"binds_to"`, `"maps"`, `"constrains"`,
    /// etc. Future filters on the FE key on this value.
    pub edge_kind: String,
    /// Axis that owns `target_id`.
    pub target_axis: Axis,
    pub target_kind: String,
    pub target_id: String,
}

#[utoipa::path(
    get,
    path = "/api/ontologies/{id}/cross-refs",
    params(("id" = Uuid, Path, description = "Ontology identity id")),
    responses(
        (status = 200, description = "Cross-axis reference edges", body = Object),
        (status = 404, description = "Ontology not found or has no committed version"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn list_cross_refs(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<CrossRefEdge>>>, AppError> {
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
        .load_version(current.id)
        .await
        .map_err(AppError::from)?;

    let mut edges = Vec::new();
    emit_edges(&ir, &mut edges);
    Ok(ApiResponse::of(edges))
}

/// Walk the IR once collecting every cross-reference pointer the
/// Complete Map surfaces. Each `push` below names one logical
/// relation — add new pointer walks at the end of this function
/// and the wire contract extends without reshuffling existing
/// edges.
fn emit_edges(ir: &ox_ontology::OntologyIR, out: &mut Vec<CrossRefEdge>) {
    // --- Topology internal edges (node_type → node_type) ---------
    for node in ir.node_types() {
        if let Some(parent) = &node.parent {
            out.push(CrossRefEdge {
                source_axis: Axis::Topology,
                source_kind: "node_type".into(),
                source_id: node.id.as_str().into(),
                edge_kind: "parent".into(),
                target_axis: Axis::Topology,
                target_kind: "node_type".into(),
                target_id: parent.as_str().into(),
            });
        }
        if let Some(replaced_by) = &node.replaced_by_id {
            out.push(CrossRefEdge {
                source_axis: Axis::Topology,
                source_kind: "node_type".into(),
                source_id: node.id.as_str().into(),
                edge_kind: "replaced_by".into(),
                target_axis: Axis::Topology,
                target_kind: "node_type".into(),
                target_id: replaced_by.as_str().into(),
            });
        }
        // PropertyDef pointers — property is nested under a node,
        // so source_id encodes the owner so the FE can group.
        for prop in &node.properties {
            emit_property_edges(
                "node",
                node.id.as_str(),
                prop,
                out,
            );
        }
    }

    // --- Topology edges (edge_type → node_type, edge_type → edge_type) ---
    for edge in ir.edge_types() {
        out.push(CrossRefEdge {
            source_axis: Axis::Topology,
            source_kind: "edge_type".into(),
            source_id: edge.id.as_str().into(),
            edge_kind: "source".into(),
            target_axis: Axis::Topology,
            target_kind: "node_type".into(),
            target_id: edge.source_node_id.as_str().into(),
        });
        out.push(CrossRefEdge {
            source_axis: Axis::Topology,
            source_kind: "edge_type".into(),
            source_id: edge.id.as_str().into(),
            edge_kind: "target".into(),
            target_axis: Axis::Topology,
            target_kind: "node_type".into(),
            target_id: edge.target_node_id.as_str().into(),
        });
        for prop in &edge.properties {
            emit_property_edges(
                "edge",
                edge.id.as_str(),
                prop,
                out,
            );
        }
    }

    // --- VOL mappings (Axis 1 ↔ Axis 5) --------------------------
    for om in ir.object_mappings() {
        out.push(CrossRefEdge {
            source_axis: Axis::Vol,
            source_kind: "object_mapping".into(),
            source_id: om.id.as_str().into(),
            edge_kind: "maps".into(),
            target_axis: Axis::Topology,
            target_kind: "node_type".into(),
            target_id: om.node_type_id.as_str().into(),
        });
    }
    for lm in ir.link_mappings() {
        out.push(CrossRefEdge {
            source_axis: Axis::Vol,
            source_kind: "link_mapping".into(),
            source_id: lm.id.as_str().into(),
            edge_kind: "maps".into(),
            target_axis: Axis::Topology,
            target_kind: "edge_type".into(),
            target_id: lm.edge_type_id.as_str().into(),
        });
    }

    // --- Registry internal (ValueSet → CodeSystem, ConceptMap →
    //                        CodeSystem × 2) --------------------
    for vs in ir.value_sets() {
        for inc in &vs.composition {
            out.push(CrossRefEdge {
                source_axis: Axis::Registry,
                source_kind: "value_set".into(),
                source_id: vs.id.as_str().into(),
                edge_kind: "includes_from".into(),
                target_axis: Axis::Registry,
                target_kind: "code_system".into(),
                target_id: inc.system_id.as_str().into(),
            });
        }
    }
    for cm in ir.concept_maps() {
        out.push(CrossRefEdge {
            source_axis: Axis::Registry,
            source_kind: "concept_map".into(),
            source_id: cm.id.as_str().into(),
            edge_kind: "source_system".into(),
            target_axis: Axis::Registry,
            target_kind: "code_system".into(),
            target_id: cm.source_system_id.as_str().into(),
        });
        out.push(CrossRefEdge {
            source_axis: Axis::Registry,
            source_kind: "concept_map".into(),
            source_id: cm.id.as_str().into(),
            edge_kind: "target_system".into(),
            target_axis: Axis::Registry,
            target_kind: "code_system".into(),
            target_id: cm.target_system_id.as_str().into(),
        });
    }

    // --- Strategy (Rules point into Topology / Registry) ---------
    for rule in ir.rules() {
        // The specific constraint-kind shapes are intentionally
        // under-covered here — the `Phase-1 RegistryReferenceCheck`
        // already enumerates them for dangling-ref reporting, and
        // this view wants a coarser "rule constrains X" link per
        // rule rather than per constraint. Callers that want the
        // constraint-level detail can subscribe to /map-summary's
        // danglers output alongside.
        //
        // For now emit one edge per scope the rule's RuleKind
        // names. `ScopeSummary` is a light helper that keeps this
        // function from blowing up into a match-on-every-variant.
        let scope = rule_scope_summary(rule);
        for node_id in scope.node_type_ids {
            out.push(CrossRefEdge {
                source_axis: Axis::Strategy,
                source_kind: "rule".into(),
                source_id: rule.id.as_str().into(),
                edge_kind: "constrains".into(),
                target_axis: Axis::Topology,
                target_kind: "node_type".into(),
                target_id: node_id,
            });
        }
        for edge_id in scope.edge_type_ids {
            out.push(CrossRefEdge {
                source_axis: Axis::Strategy,
                source_kind: "rule".into(),
                source_id: rule.id.as_str().into(),
                edge_kind: "constrains".into(),
                target_axis: Axis::Topology,
                target_kind: "edge_type".into(),
                target_id: edge_id,
            });
        }
    }

    // --- Glossary hierarchy (Vocabulary internal) ---------------
    for g in ir.glossary() {
        if let Some(parent) = &g.parent_term_id {
            out.push(CrossRefEdge {
                source_axis: Axis::Vocabulary,
                source_kind: "glossary_term".into(),
                source_id: g.id.as_str().into(),
                edge_kind: "parent".into(),
                target_axis: Axis::Vocabulary,
                target_kind: "glossary_term".into(),
                target_id: parent.as_str().into(),
            });
        }
    }
}

/// Emit every pointer-field edge a single PropertyDef owns. The
/// caller supplies the owner kind (`"node"` / `"edge"`) and owner
/// id so the source id is unique across the whole IR —
/// `"node:Customer/tier"` vs `"edge:PLACED/tier"` for identically
/// named properties on two different types.
fn emit_property_edges(
    owner_kind: &str,
    owner_id: &str,
    prop: &ox_ontology::ir::PropertyDef,
    out: &mut Vec<CrossRefEdge>,
) {
    let source_id = format!("{owner_kind}:{owner_id}/{}", prop.id.as_str());
    if let Some(gid) = &prop.glossary_term_id {
        out.push(CrossRefEdge {
            source_axis: Axis::Topology,
            source_kind: "property".into(),
            source_id: source_id.clone(),
            edge_kind: "binds_to".into(),
            target_axis: Axis::Vocabulary,
            target_kind: "glossary_term".into(),
            target_id: gid.as_str().into(),
        });
    }
    if let Some(vid) = &prop.value_set_id {
        out.push(CrossRefEdge {
            source_axis: Axis::Topology,
            source_kind: "property".into(),
            source_id: source_id.clone(),
            edge_kind: "values_in".into(),
            target_axis: Axis::Registry,
            target_kind: "value_set".into(),
            target_id: vid.as_str().into(),
        });
    }
    if let Some(nid) = &prop.notation_pattern_id {
        out.push(CrossRefEdge {
            source_axis: Axis::Topology,
            source_kind: "property".into(),
            source_id: source_id.clone(),
            edge_kind: "matches".into(),
            target_axis: Axis::Registry,
            target_kind: "notation_pattern".into(),
            target_id: nid.as_str().into(),
        });
    }
}

/// Best-effort collector for "which node/edge types does this rule
/// reach into?". Matches the current `RuleKind` variants — a new
/// variant that introduces a scope the match doesn't handle
/// silently drops out of this view (the dangling-ref surface in
/// `/map-summary` catches any orphan ids separately).
fn rule_scope_summary(rule: &ox_ontology::RuleDef) -> ScopeSummary {
    use ox_ontology::rule::RuleKind;
    let mut out = ScopeSummary::default();
    match &rule.kind {
        RuleKind::NodeShape { target_node_type_id } => {
            out.node_type_ids.push(target_node_type_id.as_str().into());
        }
        RuleKind::PropertyShape {
            target_node_type_id,
            ..
        } => {
            out.node_type_ids.push(target_node_type_id.as_str().into());
        }
        RuleKind::EdgeShape { target_edge_type_id } => {
            out.edge_type_ids.push(target_edge_type_id.as_str().into());
        }
        RuleKind::StateMachine {
            target_node_type_id,
            ..
        } => {
            out.node_type_ids.push(target_node_type_id.as_str().into());
        }
        // CrossEntityShape carries a free-form predicate; no typed
        // scope to extract, so the rule becomes a "floating" node
        // on the FE graph.
        RuleKind::CrossEntityShape { .. } => {}
    }
    out
}

#[derive(Default)]
struct ScopeSummary {
    node_type_ids: Vec<String>,
    edge_type_ids: Vec<String>,
}

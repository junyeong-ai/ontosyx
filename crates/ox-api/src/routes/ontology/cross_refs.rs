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
        .get_ontology_ir(current.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;

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
        // Kind-level scope: most rules name a target node/edge on
        // their `RuleKind` variant. Extracted via `ScopeSummary` so
        // adding a new RuleKind variant surfaces at the match site.
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

        // Constraint-level pointers into Registry — `InValueSet` and
        // `MatchesPattern` carry ids; the FE shows "this rule
        // references ValueSet X" arrows so a registry edit can
        // surface the rules that depend on it.
        use ox_ontology::rule::ShaclConstraint;
        for constraint in &rule.constraints {
            match constraint {
                ShaclConstraint::InValueSet { value_set_id, .. } => {
                    out.push(CrossRefEdge {
                        source_axis: Axis::Strategy,
                        source_kind: "rule".into(),
                        source_id: rule.id.as_str().into(),
                        edge_kind: "references_value_set".into(),
                        target_axis: Axis::Registry,
                        target_kind: "value_set".into(),
                        target_id: value_set_id.as_str().into(),
                    });
                }
                ShaclConstraint::MatchesPattern {
                    notation_pattern_id,
                    ..
                } => {
                    out.push(CrossRefEdge {
                        source_axis: Axis::Strategy,
                        source_kind: "rule".into(),
                        source_id: rule.id.as_str().into(),
                        edge_kind: "references_pattern".into(),
                        target_axis: Axis::Registry,
                        target_kind: "notation_pattern".into(),
                        target_id: notation_pattern_id.as_str().into(),
                    });
                }
                // Every other constraint kind is either intrinsic
                // (MinCount, MaxLength, ...) or references types
                // the scope summary already covered. Skipping here
                // keeps the emitted edges precise — "arrow exists
                // because of a pointer field", not "arrow exists
                // because a rule mentions a column".
                _ => {}
            }
        }
    }

    // --- Glossary SKOS relations (Vocabulary internal) ----------
    // Each `related_terms` entry surfaces as one cross-ref edge so
    // the FE can render the SKOS network without re-walking the IR.
    for g in ir.glossary() {
        for rel in &g.related_terms {
            out.push(CrossRefEdge {
                source_axis: Axis::Vocabulary,
                source_kind: "glossary_term".into(),
                source_id: g.id.as_str().into(),
                edge_kind: serde_json::to_value(rel.kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "related".to_string()),
                target_axis: Axis::Vocabulary,
                target_kind: "glossary_term".into(),
                target_id: rel.target.as_str().into(),
            });
        }
    }

    // --- Node-type pointers into Vocabulary + Strategy ----------
    //
    // `implements` lifts topology up to vocabulary (interfaces),
    // while `actions`, `metrics`, `rules` are convenience indexes
    // into strategy collections the node chooses to expose. Each
    // vector can be empty, so iterating is a no-op when the node
    // hasn't opted in.
    for node in ir.node_types() {
        for iface in &node.implements {
            out.push(CrossRefEdge {
                source_axis: Axis::Topology,
                source_kind: "node_type".into(),
                source_id: node.id.as_str().into(),
                edge_kind: "implements".into(),
                target_axis: Axis::Vocabulary,
                target_kind: "interface".into(),
                target_id: iface.as_str().into(),
            });
        }
        for action in &node.actions {
            out.push(CrossRefEdge {
                source_axis: Axis::Topology,
                source_kind: "node_type".into(),
                source_id: node.id.as_str().into(),
                edge_kind: "exposes".into(),
                target_axis: Axis::Strategy,
                target_kind: "action".into(),
                target_id: action.as_str().into(),
            });
        }
        for metric in &node.metrics {
            out.push(CrossRefEdge {
                source_axis: Axis::Topology,
                source_kind: "node_type".into(),
                source_id: node.id.as_str().into(),
                edge_kind: "tracks".into(),
                target_axis: Axis::Strategy,
                target_kind: "metric".into(),
                target_id: metric.as_str().into(),
            });
        }
        for rule in &node.rules {
            out.push(CrossRefEdge {
                source_axis: Axis::Topology,
                source_kind: "node_type".into(),
                source_id: node.id.as_str().into(),
                edge_kind: "governed_by".into(),
                target_axis: Axis::Strategy,
                target_kind: "rule".into(),
                target_id: rule.as_str().into(),
            });
        }
    }

    // --- ActionDef pointers -------------------------------------
    //
    // Target is always topology (node or edge). Pre/postcondition
    // rules surface the action → rule dependency both for the
    // visualiser and for a future "which actions use rule X?"
    // filter on the FE.
    for action in ir.actions() {
        use ox_ontology::action::ActionTarget;
        match &action.target {
            ActionTarget::NodeType { node_type_id } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Strategy,
                    source_kind: "action".into(),
                    source_id: action.id.as_str().into(),
                    edge_kind: "writes_to".into(),
                    target_axis: Axis::Topology,
                    target_kind: "node_type".into(),
                    target_id: node_type_id.as_str().into(),
                });
            }
            ActionTarget::EdgeType { edge_type_id } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Strategy,
                    source_kind: "action".into(),
                    source_id: action.id.as_str().into(),
                    edge_kind: "writes_to".into(),
                    target_axis: Axis::Topology,
                    target_kind: "edge_type".into(),
                    target_id: edge_type_id.as_str().into(),
                });
            }
        }
        for rule_id in &action.preconditions {
            out.push(CrossRefEdge {
                source_axis: Axis::Strategy,
                source_kind: "action".into(),
                source_id: action.id.as_str().into(),
                edge_kind: "precondition".into(),
                target_axis: Axis::Strategy,
                target_kind: "rule".into(),
                target_id: rule_id.as_str().into(),
            });
        }
        for rule_id in &action.postconditions {
            out.push(CrossRefEdge {
                source_axis: Axis::Strategy,
                source_kind: "action".into(),
                source_id: action.id.as_str().into(),
                edge_kind: "postcondition".into(),
                target_axis: Axis::Strategy,
                target_kind: "rule".into(),
                target_id: rule_id.as_str().into(),
            });
        }
    }

    // --- FunctionDef dependencies -------------------------------
    //
    // Functions depend on properties (attribute reads) and edge
    // types (traversals). Both land as `depends_on` edges since
    // the cache-invalidation trigger is identical from the FE's
    // perspective; the source_kind distinguishes edge vs. property.
    for f in ir.functions() {
        for dep in &f.property_dependencies {
            let source_id =
                format!("node:{}/{}", dep.node_type_id.as_str(), dep.property_id.as_str());
            out.push(CrossRefEdge {
                source_axis: Axis::Strategy,
                source_kind: "function".into(),
                source_id: f.id.as_str().into(),
                edge_kind: "depends_on".into(),
                target_axis: Axis::Topology,
                target_kind: "property".into(),
                target_id: source_id,
            });
        }
        for edge_id in &f.edge_dependencies {
            out.push(CrossRefEdge {
                source_axis: Axis::Strategy,
                source_kind: "function".into(),
                source_id: f.id.as_str().into(),
                edge_kind: "depends_on".into(),
                target_axis: Axis::Topology,
                target_kind: "edge_type".into(),
                target_id: edge_id.as_str().into(),
            });
        }
    }

    // --- MetricDef scope ----------------------------------------
    for metric in ir.metrics() {
        use ox_ontology::metric::MetricScope;
        match &metric.target_scope {
            MetricScope::NodeType { node_type_id } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Strategy,
                    source_kind: "metric".into(),
                    source_id: metric.id.as_str().into(),
                    edge_kind: "scopes".into(),
                    target_axis: Axis::Topology,
                    target_kind: "node_type".into(),
                    target_id: node_type_id.as_str().into(),
                });
            }
            MetricScope::EdgeType { edge_type_id } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Strategy,
                    source_kind: "metric".into(),
                    source_id: metric.id.as_str().into(),
                    edge_kind: "scopes".into(),
                    target_axis: Axis::Topology,
                    target_kind: "edge_type".into(),
                    target_id: edge_type_id.as_str().into(),
                });
            }
            MetricScope::Global => {
                // A global metric aggregates across the whole
                // ontology; no typed target — it becomes a
                // "floating" metric on the FE graph.
            }
        }
    }

    // --- EnrichmentDef pointers ---------------------------------
    //
    // Every enrichment lands three edges: target node type, join
    // key property (implicitly co-owned by the target), target
    // property (same). Property source_ids use the owner-prefix
    // convention so they're unique across the IR.
    for e in ir.enrichments() {
        let owner_id = e.target_node_type_id.as_str();
        out.push(CrossRefEdge {
            source_axis: Axis::Governance,
            source_kind: "enrichment".into(),
            source_id: e.id.as_str().into(),
            edge_kind: "enriches".into(),
            target_axis: Axis::Topology,
            target_kind: "node_type".into(),
            target_id: owner_id.into(),
        });
        out.push(CrossRefEdge {
            source_axis: Axis::Governance,
            source_kind: "enrichment".into(),
            source_id: e.id.as_str().into(),
            edge_kind: "join_key".into(),
            target_axis: Axis::Topology,
            target_kind: "property".into(),
            target_id: format!("node:{}/{}", owner_id, e.join_key_property_id.as_str()),
        });
        out.push(CrossRefEdge {
            source_axis: Axis::Governance,
            source_kind: "enrichment".into(),
            source_id: e.id.as_str().into(),
            edge_kind: "writes".into(),
            target_axis: Axis::Topology,
            target_kind: "property".into(),
            target_id: format!("node:{}/{}", owner_id, e.target_property_id.as_str()),
        });
    }

    // --- InterfaceDef requirements ------------------------------
    //
    // Interfaces declare *expected* shapes — their required_*
    // vectors may pin a specific id or match by name. Only the
    // pinned ones produce hard pointer edges; name-only
    // requirements stay invisible here because the matching is
    // structural, not referential.
    for iface in ir.interfaces() {
        for req in &iface.required_properties {
            if let Some(pid) = &req.expected_property_id {
                out.push(CrossRefEdge {
                    source_axis: Axis::Vocabulary,
                    source_kind: "interface".into(),
                    source_id: iface.id.as_str().into(),
                    edge_kind: "requires_property".into(),
                    target_axis: Axis::Topology,
                    target_kind: "property_id".into(),
                    target_id: pid.as_str().into(),
                });
            }
        }
        for req in &iface.required_edges {
            if let Some(eid) = &req.expected_edge_type_id {
                out.push(CrossRefEdge {
                    source_axis: Axis::Vocabulary,
                    source_kind: "interface".into(),
                    source_id: iface.id.as_str().into(),
                    edge_kind: "requires_edge".into(),
                    target_axis: Axis::Topology,
                    target_kind: "edge_type".into(),
                    target_id: eid.as_str().into(),
                });
            }
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
    // Single walk over the property's binding list — every cross-
    // reference edge is emitted from the same loop so a future
    // PropertyBinding variant only adds one match arm.
    for binding in &prop.bindings {
        match binding {
            ox_ontology::PropertyBinding::Glossary { id, .. } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Topology,
                    source_kind: "property".into(),
                    source_id: source_id.clone(),
                    edge_kind: "binds_to".into(),
                    target_axis: Axis::Vocabulary,
                    target_kind: "glossary_term".into(),
                    target_id: id.as_str().into(),
                });
            }
            ox_ontology::PropertyBinding::ValueSet { id, .. } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Topology,
                    source_kind: "property".into(),
                    source_id: source_id.clone(),
                    edge_kind: "values_in".into(),
                    target_axis: Axis::Registry,
                    target_kind: "value_set".into(),
                    target_id: id.as_str().into(),
                });
            }
            ox_ontology::PropertyBinding::NotationPattern { id, .. } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Topology,
                    source_kind: "property".into(),
                    source_id: source_id.clone(),
                    edge_kind: "matches".into(),
                    target_axis: Axis::Registry,
                    target_kind: "notation_pattern".into(),
                    target_id: id.as_str().into(),
                });
            }
            ox_ontology::PropertyBinding::ValueRange { id, .. } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Topology,
                    source_kind: "property".into(),
                    source_id: source_id.clone(),
                    edge_kind: "classified_by".into(),
                    target_axis: Axis::Registry,
                    target_kind: "value_range_set".into(),
                    target_id: id.as_str().into(),
                });
            }
            ox_ontology::PropertyBinding::CodeSystem { id, .. } => {
                out.push(CrossRefEdge {
                    source_axis: Axis::Topology,
                    source_kind: "property".into(),
                    source_id: source_id.clone(),
                    edge_kind: "values_in".into(),
                    target_axis: Axis::Registry,
                    target_kind: "code_system".into(),
                    target_id: id.as_str().into(),
                });
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::{GraphLabel, PropertyKey};
    use ox_ontology::glossary::{GlossaryTermDef, GlossaryTermId};
    use ox_ontology::ir::{
        EdgeTypeDef, EdgeTypeId, NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId,
    };
    use ox_ontology::value_set::ValueSetId;

    fn gl(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("valid label")
    }

    fn pk(s: &str) -> PropertyKey {
        PropertyKey::new(s).expect("valid key")
    }

    // --- helpers to find emitted edges by their (source_kind,
    //     edge_kind, target_kind) triple. Keep the test assertions
    //     readable.
    fn count_where(
        edges: &[CrossRefEdge],
        source_kind: &str,
        edge_kind: &str,
        target_kind: &str,
    ) -> usize {
        edges
            .iter()
            .filter(|e| {
                e.source_kind == source_kind
                    && e.edge_kind == edge_kind
                    && e.target_kind == target_kind
            })
            .count()
    }

    fn empty_ir() -> OntologyIR {
        OntologyIR::new(
            "ont-test".into(),
            "Test".into(),
            Default::default(),
            1u32,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn property_glossary_binding_emits_binds_to() {
        let mut ir = empty_ir();
        let prop = PropertyDef {
            id: PropertyId::new("tier"),
            name: pk("tier"),
            property_type: ox_core::types::PropertyType::String,
            bindings: vec![ox_ontology::PropertyBinding::glossary(GlossaryTermId::new("g-tier"),)],
            ..Default::default()
        };
        let node = NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            properties: vec![prop],
            ..Default::default()
        };
        ir.add_node_type(node).unwrap();
        ir.add_glossary_term(GlossaryTermDef {
            id: GlossaryTermId::new("g-tier"),
            term: ox_core::i18n::LocalizedText::new("Tier"),
            display_name: Default::default(),
            description: Default::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: ox_ontology::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: ox_ontology::glossary::TermLifecycle::default(),
        })
        .unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        assert_eq!(
            count_where(&edges, "property", "binds_to", "glossary_term"),
            1,
        );
    }

    #[test]
    fn property_value_set_pointer_emits_values_in() {
        let mut ir = empty_ir();
        let prop = PropertyDef {
            id: PropertyId::new("country"),
            name: pk("country"),
            property_type: ox_core::types::PropertyType::String,
            bindings: vec![ox_ontology::PropertyBinding::value_set(ValueSetId::new("v-iso"),)],
            ..Default::default()
        };
        let node = NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            properties: vec![prop],
            ..Default::default()
        };
        ir.add_node_type(node).unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        assert_eq!(
            count_where(&edges, "property", "values_in", "value_set"),
            1,
        );
    }

    #[test]
    fn edge_type_source_and_target_both_emit_topology_edges() {
        let mut ir = empty_ir();
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            ..Default::default()
        })
        .unwrap();
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Order"),
            label: gl("Order"),
            ..Default::default()
        })
        .unwrap();
        ir.add_edge_type(EdgeTypeDef {
            id: EdgeTypeId::new("PLACED"),
            label: gl("PLACED"),
            source_node_id: NodeTypeId::new("Customer"),
            target_node_id: NodeTypeId::new("Order"),
            ..Default::default()
        })
        .unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        assert_eq!(count_where(&edges, "edge_type", "source", "node_type"), 1);
        assert_eq!(count_where(&edges, "edge_type", "target", "node_type"), 1);
    }

    #[test]
    fn empty_ir_emits_no_edges() {
        let ir = empty_ir();
        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        assert!(edges.is_empty());
    }

    #[test]
    fn property_with_no_pointers_emits_no_edges() {
        let mut ir = empty_ir();
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            properties: vec![PropertyDef {
                id: PropertyId::new("tier"),
                name: pk("tier"),
                property_type: ox_core::types::PropertyType::String,
                ..Default::default()
            }],
            ..Default::default()
        })
        .unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        // Only the node exists — no property pointers, no edges,
        // no mappings — so emit_edges produces nothing.
        assert!(edges.is_empty());
    }

    #[test]
    fn rule_in_value_set_constraint_emits_references_value_set() {
        use ox_ontology::rule::{ConstraintTarget, RuleKind, ShaclConstraint};
        use ox_ontology::{RuleDef, RuleId, value_set::ValueSetId};
        let mut ir = empty_ir();
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            ..Default::default()
        })
        .unwrap();
        ir.add_rule(RuleDef {
            id: RuleId::new("r-country"),
            name: "Country must be ISO-2".into(),
            description: Default::default(),
            rationale: Default::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("Customer"),
            },
            severity: Default::default(),
            enforcement: Default::default(),
            activation: Default::default(),
            origin: Default::default(),
            constraints: vec![ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: ValueSetId::new("v-iso"),
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        assert_eq!(
            count_where(
                &edges,
                "rule",
                "references_value_set",
                "value_set",
            ),
            1,
        );
        // The kind-level `constrains` edge still fires too.
        assert_eq!(
            count_where(&edges, "rule", "constrains", "node_type"),
            1,
        );
    }

    #[test]
    fn rule_matches_pattern_constraint_emits_references_pattern() {
        use ox_ontology::notation_pattern::NotationPatternId;
        use ox_ontology::rule::{ConstraintTarget, RuleKind, ShaclConstraint};
        use ox_ontology::{RuleDef, RuleId};
        let mut ir = empty_ir();
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            ..Default::default()
        })
        .unwrap();
        ir.add_rule(RuleDef {
            id: RuleId::new("r-email"),
            name: "Email must match RFC 5322".into(),
            description: Default::default(),
            rationale: Default::default(),
            kind: RuleKind::NodeShape {
                target_node_type_id: NodeTypeId::new("Customer"),
            },
            severity: Default::default(),
            enforcement: Default::default(),
            activation: Default::default(),
            origin: Default::default(),
            constraints: vec![ShaclConstraint::MatchesPattern {
                target: ConstraintTarget::Inherit,
                notation_pattern_id: NotationPatternId::new("p-email"),
            }],
                    valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        assert_eq!(
            count_where(
                &edges,
                "rule",
                "references_pattern",
                "notation_pattern",
            ),
            1,
        );
    }

    #[test]
    fn property_source_id_encodes_owner_type() {
        let mut ir = empty_ir();
        let prop = PropertyDef {
            id: PropertyId::new("name"),
            name: pk("name"),
            property_type: ox_core::types::PropertyType::String,
            bindings: vec![ox_ontology::PropertyBinding::glossary(GlossaryTermId::new("g-name"),)],
            ..Default::default()
        };
        // Same property id on a node and on an edge — the compound
        // source_id is what keeps them distinct in the emitted
        // edge list.
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: gl("Customer"),
            properties: vec![prop.clone()],
            ..Default::default()
        })
        .unwrap();
        ir.add_node_type(NodeTypeDef {
            id: NodeTypeId::new("Order"),
            label: gl("Order"),
            ..Default::default()
        })
        .unwrap();
        ir.add_edge_type(EdgeTypeDef {
            id: EdgeTypeId::new("PLACED"),
            label: gl("PLACED"),
            source_node_id: NodeTypeId::new("Customer"),
            target_node_id: NodeTypeId::new("Order"),
            properties: vec![prop],
            ..Default::default()
        })
        .unwrap();
        ir.add_glossary_term(GlossaryTermDef {
            id: GlossaryTermId::new("g-name"),
            term: ox_core::i18n::LocalizedText::new("Name"),
            display_name: Default::default(),
            description: Default::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: ox_ontology::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: ox_ontology::glossary::TermLifecycle::default(),
        })
        .unwrap();

        let mut edges = Vec::new();
        emit_edges(&ir, &mut edges);
        let prop_edges: Vec<&CrossRefEdge> = edges
            .iter()
            .filter(|e| e.edge_kind == "binds_to")
            .collect();
        assert_eq!(prop_edges.len(), 2);
        assert!(prop_edges
            .iter()
            .any(|e| e.source_id == "node:Customer/name"));
        assert!(prop_edges
            .iter()
            .any(|e| e.source_id == "edge:PLACED/name"));
    }
}

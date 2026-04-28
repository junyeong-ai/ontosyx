//! `MatchPlanner` — resolve a `QueryOp::Match` into a
//! backend-agnostic plan spec.
//!
//! Sits between the "label → mapping" stages (LabelResolver,
//! MappingResolver) and the eventual `LogicalPlanBuilder`. Given a
//! `Match` op and an `OntologyIR` snapshot it returns a
//! [`MatchPlanSpec`] — a pure data structure that names:
//!
//! - every bound variable in the match,
//! - the concrete node types each variable binds to,
//! - the object mappings applicable for each (`precedence`-sorted),
//! - the hop structure (source → edge → target) between variables.
//!
//! DataFusion is intentionally absent from this module. A future
//! `LogicalPlanBuilder` turns `MatchPlanSpec` into a `LogicalPlan`;
//! keeping the planner layer pure lets us test the shape of the
//! match independently of the engine.
//!
//! ## Phase 6-C slice 1 scope
//!
//! - `GraphPattern::Node` — fully handled. Every node pattern
//!   contributes a `NodeScanSpec`.
//! - `GraphPattern::Relationship` — recorded as a `HopSpec` that
//!   references the `source` / `target` variables already bound
//!   by sibling Node patterns. The edge's own label (if any) is
//!   resolved against the ontology's edge types.
//! - `GraphPattern::Path` — explicitly rejected with an
//!   `Unsupported` error. Path patterns decompose into node / edge
//!   sequences; that decomposition lands in Phase 6-C slice 2
//!   (see `docs/adr/0002-datafusion-federation.md` for the overall
//!   staging).

use chrono::{DateTime, Utc};

use ox_core::graph_label::GraphLabel;
use ox_core::types::Direction;
use ox_core::variable_name::VariableName;
use ox_ontology::OntologyIR;
use ox_ontology::ir::{EdgeTypeId, NodeTypeId};
use ox_ontology::mapping::{LinkMappingDef, ObjectMappingDef};
use ox_query_ir::query::{GraphPattern, QueryOp};

use crate::error::{FederationError, FederationResult};
use crate::planner::label_resolver::{LabelResolver, ResolvedLabelTarget};
use crate::planner::mapping_resolver::MappingResolver;

/// Full plan for a single `QueryOp::Match`.
#[derive(Debug, Clone)]
pub struct MatchPlanSpec<'a> {
    /// One entry per bound node variable, in declaration order.
    pub scans: Vec<NodeScanSpec<'a>>,
    /// One entry per relationship pattern. References variables
    /// declared in `scans` by name — the planner does not
    /// re-resolve them per hop.
    pub hops: Vec<HopSpec<'a>>,
}

impl<'a> MatchPlanSpec<'a> {
    pub fn is_empty(&self) -> bool {
        self.scans.is_empty() && self.hops.is_empty()
    }
}

/// Plan for one bound node variable.
#[derive(Debug, Clone)]
pub struct NodeScanSpec<'a> {
    pub variable: VariableName,
    /// `None` when the pattern is label-less (`MATCH (n)`). The
    /// federation planner still needs a scan entry so downstream
    /// `HopSpec` can reference `n`, but it must materialise a
    /// union of every mapped node type — a Phase 6-C slice 3
    /// concern, not this module's.
    pub target: Option<ResolvedLabelTarget>,
    /// Flat list of `(node_type, mapping)` pairs across every
    /// implementer of the target (or just the concrete type).
    /// Empty when the target is an interface with no implementers,
    /// or a concrete node type with no mappings — the downstream
    /// builder treats both as "no scan produces rows for this
    /// variable" and emits an empty plan.
    pub mappings: Vec<ScanMappingEntry<'a>>,
}

/// One node-type × object-mapping pair inside a `NodeScanSpec`.
#[derive(Debug, Clone)]
pub struct ScanMappingEntry<'a> {
    pub node_type_id: NodeTypeId,
    pub mapping: &'a ObjectMappingDef,
}

/// Plan for one relationship pattern.
#[derive(Debug, Clone)]
pub struct HopSpec<'a> {
    pub source_variable: VariableName,
    pub target_variable: VariableName,
    pub direction: Direction,
    /// The edge label, if the pattern named one. `None` → match
    /// every edge type between the two endpoints.
    pub edge_label: Option<GraphLabel>,
    /// Resolved edge type + applicable link mappings, when the
    /// label pinned one. `Vec::is_empty()` with `edge_label = Some(_)`
    /// means "label is known but no link mapping exists" — the
    /// planner warns and emits an empty plan.
    pub link_mappings: Vec<HopMappingEntry<'a>>,
}

/// One edge-type × link-mapping pair inside a `HopSpec`.
#[derive(Debug, Clone)]
pub struct HopMappingEntry<'a> {
    pub edge_type_id: EdgeTypeId,
    pub mapping: &'a LinkMappingDef,
}

/// Pure-function planner for a single match operation.
#[derive(Debug, Clone)]
pub struct MatchPlanner<'a> {
    ontology: &'a OntologyIR,
    at: Option<DateTime<Utc>>,
}

impl<'a> MatchPlanner<'a> {
    pub fn new(ontology: &'a OntologyIR) -> Self {
        Self { ontology, at: None }
    }

    pub fn at(ontology: &'a OntologyIR, at: DateTime<Utc>) -> Self {
        Self {
            ontology,
            at: Some(at),
        }
    }

    /// Plan the given `Match` op. Non-`Match` ops are rejected —
    /// the planner is intentionally single-purpose so the caller
    /// routes each op to its specialised planner.
    pub fn plan(&self, op: &QueryOp) -> FederationResult<MatchPlanSpec<'a>> {
        let patterns = match op {
            QueryOp::Match { patterns, .. } => patterns,
            _ => {
                return Err(FederationError::unsupported(
                    "MatchPlanner: only QueryOp::Match is accepted",
                ));
            }
        };

        let label_resolver = LabelResolver::new(self.ontology);
        let mapping_resolver = match self.at {
            Some(t) => MappingResolver::at(self.ontology, t),
            None => MappingResolver::new(self.ontology),
        };

        let mut scans: Vec<NodeScanSpec<'a>> = Vec::new();
        let mut hops: Vec<HopSpec<'a>> = Vec::new();

        for pattern in patterns {
            match pattern {
                GraphPattern::Node {
                    variable,
                    label,
                    property_filters: _,
                } => {
                    let (target, mappings) = self.plan_node(
                        label.as_ref(),
                        &label_resolver,
                        &mapping_resolver,
                    )?;
                    scans.push(NodeScanSpec {
                        variable: variable.clone(),
                        target,
                        mappings,
                    });
                }
                GraphPattern::Relationship {
                    variable: _,
                    label,
                    source,
                    target,
                    direction,
                    property_filters: _,
                    var_length,
                } => {
                    if var_length.is_some() {
                        return Err(FederationError::unsupported(
                            "MatchPlanner: variable-length relationship patterns are \
                             not yet supported (Phase 6-C slice 2)",
                        ));
                    }
                    let link_mappings = match label {
                        Some(l) => self.plan_edge(l, &mapping_resolver)?,
                        None => Vec::new(),
                    };
                    hops.push(HopSpec {
                        source_variable: source.clone(),
                        target_variable: target.clone(),
                        direction: *direction,
                        edge_label: label.clone(),
                        link_mappings,
                    });
                }
                GraphPattern::Path { .. } => {
                    return Err(FederationError::unsupported(
                        "MatchPlanner: path patterns are not yet supported \
                         (Phase 6-C slice 2 decomposes them into node + edge sequences)",
                    ));
                }
            }
        }

        Ok(MatchPlanSpec { scans, hops })
    }

    fn plan_node(
        &self,
        label: Option<&GraphLabel>,
        label_resolver: &LabelResolver<'a>,
        mapping_resolver: &MappingResolver<'a>,
    ) -> FederationResult<(Option<ResolvedLabelTarget>, Vec<ScanMappingEntry<'a>>)> {
        let Some(lbl) = label else {
            // Label-less pattern: the planner cannot derive a scan
            // from the ontology alone. Record the variable with no
            // target; downstream stages decide whether to emit a
            // union-of-all or reject.
            return Ok((None, Vec::new()));
        };

        let target = label_resolver.resolve(lbl)?;
        if target.is_ambiguous() {
            return Err(FederationError::unsupported(format!(
                "MatchPlanner: label '{lbl}' is ambiguous (both a node type and \
                 an interface in this ontology) — ontology editor must resolve"
            )));
        }

        let mut entries: Vec<ScanMappingEntry<'a>> = Vec::new();
        for node_type_id in target.node_type_ids() {
            let resolved = mapping_resolver.resolve_node_type(node_type_id)?;
            for mapping in resolved.mappings {
                entries.push(ScanMappingEntry {
                    node_type_id: node_type_id.clone(),
                    mapping,
                });
            }
        }
        Ok((Some(target), entries))
    }

    fn plan_edge(
        &self,
        label: &GraphLabel,
        mapping_resolver: &MappingResolver<'a>,
    ) -> FederationResult<Vec<HopMappingEntry<'a>>> {
        // Edge labels do not route through the interface path —
        // there is no `EdgeInterface` in v3 today. Match by label
        // directly against the edge types.
        let candidates: Vec<&ox_ontology::ir::EdgeTypeDef> = self
            .ontology
            .edge_types()
            .iter()
            .filter(|e| &e.label == label)
            .collect();
        if candidates.is_empty() {
            return Err(FederationError::unsupported(format!(
                "MatchPlanner: edge label '{label}' is not declared on the ontology"
            )));
        }

        let mut entries: Vec<HopMappingEntry<'a>> = Vec::new();
        for edge in candidates {
            let link_mappings = mapping_resolver.resolve_edge_type(&edge.id)?;
            for mapping in link_mappings {
                entries.push(HopMappingEntry {
                    edge_type_id: edge.id.clone(),
                    mapping,
                });
            }
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::interface::{InterfaceDef, InterfaceId};
    use ox_ontology::ir::{EdgeTypeDef, NodeTypeDef};
    use ox_ontology::mapping::ObjectMappingDef;
    use ox_query_ir::query::{QUERY_IR_SCHEMA_VERSION, QueryIR, QueryOp};

    fn gl(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("valid")
    }

    fn vn(s: &str) -> VariableName {
        VariableName::new(s).expect("valid")
    }

    fn node(id: &str, label: &str, implements: Vec<&str>) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: gl(label),
            implements: implements.into_iter().map(InterfaceId::new).collect(),
            ..Default::default()
        }
    }

    fn match_single_node(var: &str, label: Option<&str>) -> QueryOp {
        QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn(var),
                label: label.map(gl),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        }
    }

    #[test]
    fn plan_concrete_node_with_single_mapping() {
        let mut ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![node("nt-user", "User", vec![])],
            vec![],
            vec![],
        );
        ont.add_object_mapping(ObjectMappingDef::new("om-1", "nt-user", "pg", "users"))
            .unwrap();

        let planner = MatchPlanner::new(&ont);
        let spec = planner.plan(&match_single_node("n", Some("User"))).unwrap();
        assert_eq!(spec.scans.len(), 1);
        assert_eq!(spec.scans[0].variable.as_str(), "n");
        assert_eq!(spec.scans[0].mappings.len(), 1);
        assert_eq!(spec.scans[0].mappings[0].mapping.relation, "users");
        assert!(spec.hops.is_empty());
    }

    #[test]
    fn plan_interface_expands_into_every_implementer_mapping() {
        let mut ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![
                node("nt-user", "User", vec!["if-addr"]),
                node("nt-org", "Org", vec!["if-addr"]),
            ],
            vec![],
            vec![],
        );
        ont.add_interface(InterfaceDef {
            id: InterfaceId::new("if-addr"),
            label: gl("HasAddress"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            required_properties: vec![],
            required_edges: vec![],
        })
        .unwrap();
        ont.add_object_mapping(ObjectMappingDef::new("om-u", "nt-user", "pg", "users"))
            .unwrap();
        ont.add_object_mapping(ObjectMappingDef::new("om-o", "nt-org", "pg", "orgs"))
            .unwrap();

        let planner = MatchPlanner::new(&ont);
        let spec = planner
            .plan(&match_single_node("n", Some("HasAddress")))
            .unwrap();
        assert_eq!(spec.scans.len(), 1);
        assert_eq!(
            spec.scans[0].mappings.len(),
            2,
            "interface expansion must surface both implementers' mappings"
        );
        // Both implementer ids are present.
        let ids: Vec<&str> = spec.scans[0]
            .mappings
            .iter()
            .map(|e| e.node_type_id.as_str())
            .collect();
        assert!(ids.contains(&"nt-user") && ids.contains(&"nt-org"));
    }

    #[test]
    fn plan_label_less_node_records_an_empty_scan_entry() {
        let ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![node("nt-user", "User", vec![])],
            vec![],
            vec![],
        );
        let planner = MatchPlanner::new(&ont);
        let spec = planner.plan(&match_single_node("n", None)).unwrap();
        assert_eq!(spec.scans.len(), 1);
        assert!(spec.scans[0].target.is_none());
        assert!(spec.scans[0].mappings.is_empty());
    }

    #[test]
    fn plan_relationship_resolves_edge_label_to_link_mappings() {
        use ox_ontology::mapping::{
            ColumnRef, EndpointRef, JoinCostHint, LinkMappingDef, LinkMappingId, LinkMappingKind,
            SourceId,
        };

        let mut ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![
                node("nt-u", "User", vec![]),
                node("nt-o", "Order", vec![]),
            ],
            vec![EdgeTypeDef {
                id: "e-placed".into(),
                label: gl("PLACED"),
                source_node_id: "nt-u".into(),
                target_node_id: "nt-o".into(),
                ..Default::default()
            }],
            vec![],
        );
        ont.add_object_mapping(ObjectMappingDef::new("om-u", "nt-u", "pg", "users"))
            .unwrap();
        ont.add_object_mapping(ObjectMappingDef::new("om-o", "nt-o", "pg", "orders"))
            .unwrap();
        ont.add_link_mapping(LinkMappingDef {
            id: LinkMappingId::new("lm-1"),
            edge_type_id: "e-placed".into(),
            kind: LinkMappingKind::ForeignKey {
                source_column: ColumnRef::new("orders", "user_id"),
                target_column: ColumnRef::new("users", "id"),
            },
            source_endpoint: EndpointRef {
                source_id: SourceId::new("pg"),
                relation: "users".into(),
                key_columns: vec!["id".into()],
            },
            target_endpoint: EndpointRef {
                source_id: SourceId::new("pg"),
                relation: "orders".into(),
                key_columns: vec!["id".into()],
            },
            join_cost_hint: JoinCostHint::Indexed,
            precedence: 100,
            cardinality: ox_ontology::LinkCardinality::ManyToOne,
        })
        .unwrap();

        let op = QueryOp::Match {
            patterns: vec![
                GraphPattern::Node {
                    variable: vn("u"),
                    label: Some(gl("User")),
                    property_filters: vec![],
                },
                GraphPattern::Relationship {
                    variable: None,
                    label: Some(gl("PLACED")),
                    source: vn("u"),
                    target: vn("o"),
                    direction: Direction::Outgoing,
                    property_filters: vec![],
                    var_length: None,
                },
                GraphPattern::Node {
                    variable: vn("o"),
                    label: Some(gl("Order")),
                    property_filters: vec![],
                },
            ],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        };

        let planner = MatchPlanner::new(&ont);
        let spec = planner.plan(&op).unwrap();
        assert_eq!(spec.scans.len(), 2);
        assert_eq!(spec.hops.len(), 1);
        let hop = &spec.hops[0];
        assert_eq!(hop.source_variable.as_str(), "u");
        assert_eq!(hop.target_variable.as_str(), "o");
        assert_eq!(hop.link_mappings.len(), 1);
    }

    #[test]
    fn plan_rejects_variable_length_relationships() {
        use ox_query_ir::query::VarLength;

        let ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![node("nt-u", "User", vec![])],
            vec![],
            vec![],
        );
        let op = QueryOp::Match {
            patterns: vec![GraphPattern::Relationship {
                variable: None,
                label: None,
                source: vn("a"),
                target: vn("b"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: Some(VarLength {
                    min: Some(1),
                    max: Some(3),
                }),
            }],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        };
        let planner = MatchPlanner::new(&ont);
        assert!(matches!(
            planner.plan(&op),
            Err(FederationError::Unsupported(_))
        ));
    }

    #[test]
    fn plan_rejects_path_patterns() {
        let ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![node("nt-u", "User", vec![])],
            vec![],
            vec![],
        );
        let op = QueryOp::Match {
            patterns: vec![GraphPattern::Path { elements: vec![] }],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        };
        let planner = MatchPlanner::new(&ont);
        assert!(matches!(
            planner.plan(&op),
            Err(FederationError::Unsupported(_))
        ));
    }

    #[test]
    fn plan_at_pins_resolver_to_temporal_window_filtering_expired_mappings() {
        use chrono::{Duration, Utc};

        let mut ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![node("nt-user", "User", vec![])],
            vec![],
            vec![],
        );

        // Mapping live only until "an hour ago" — expired at the
        // current pivot but legitimate at any earlier instant.
        let mut expired = ObjectMappingDef::new("om-old", "nt-user", "pg", "users_v1");
        expired.valid_to = Some(Utc::now() - Duration::hours(1));
        ont.add_object_mapping(expired).unwrap();

        // Resolver pinned to "now" must filter the expired mapping
        // out → spec carries an unmapped scan.
        let planner = MatchPlanner::at(&ont, Utc::now());
        let spec = planner
            .plan(&match_single_node("n", Some("User")))
            .unwrap();
        assert_eq!(spec.scans.len(), 1);
        assert!(
            spec.scans[0].mappings.is_empty(),
            "as_of must drop expired mapping; got {:?}",
            spec.scans[0].mappings,
        );

        // Resolver pinned to "two hours ago" sees the same mapping
        // as live — proves the planner reads `at`, not "now".
        let planner_past =
            MatchPlanner::at(&ont, Utc::now() - Duration::hours(2));
        let spec_past = planner_past
            .plan(&match_single_node("n", Some("User")))
            .unwrap();
        assert_eq!(spec_past.scans[0].mappings.len(), 1);
        assert_eq!(
            spec_past.scans[0].mappings[0].mapping.relation,
            "users_v1",
        );
    }

    #[test]
    fn plan_rejects_non_match_ops() {
        let ont = OntologyIR::new(
            "ont".into(),
            "sample".into(),
            LocalizedText::default(),
            1,
            vec![node("nt-u", "User", vec![])],
            vec![],
            vec![],
        );
        let op = QueryOp::Union {
            queries: vec![QueryIR {
                schema_version: QUERY_IR_SCHEMA_VERSION,
                operation: match_single_node("n", None),
                limit: None,
                skip: None,
                order_by: vec![],
                as_of: None,
            }],
            all: true,
        };
        let planner = MatchPlanner::new(&ont);
        assert!(matches!(
            planner.plan(&op),
            Err(FederationError::Unsupported(_))
        ));
    }
}

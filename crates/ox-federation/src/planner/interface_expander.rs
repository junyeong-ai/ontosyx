//! `InterfaceExpander` — expand a query target expressed as an
//! `InterfaceId` into the concrete node types that implement it.
//!
//! A query that says `(:HasAddress)` is asking the platform to union
//! every node type whose `implements` list contains `HasAddress`. The
//! expander walks `OntologyIR::node_types()` and returns the matching
//! ids so the downstream planner can register one `TableProvider`
//! per implementer and wire the `UNION ALL` in `LogicalPlanBuilder`.
//!
//! The expander is intentionally non-recursive. An interface today
//! cannot extend another interface — implementing relationships live
//! only between node types and interfaces. A future "interface-of-
//! interfaces" would be a deliberate schema change; until then,
//! keeping the expander linear removes an entire class of cycle /
//! fan-out concerns.

use ox_ontology::OntologyIR;
use ox_ontology::interface::InterfaceId;
use ox_ontology::ir::NodeTypeId;

use crate::error::{FederationError, FederationResult};

/// Output of [`InterfaceExpander::expand`].
#[derive(Debug, Clone)]
pub struct ExpandedTargets {
    pub interface_id: InterfaceId,
    /// Node types that implement `interface_id`, in declaration order.
    /// Declaration-order is stable (node types keep their source
    /// order in the vector) and makes multi-mapping dedup
    /// deterministic downstream.
    pub node_type_ids: Vec<NodeTypeId>,
}

impl ExpandedTargets {
    pub fn is_empty(&self) -> bool {
        self.node_type_ids.is_empty()
    }
}

/// Pure-function expander over an `OntologyIR` snapshot.
#[derive(Debug, Clone)]
pub struct InterfaceExpander<'a> {
    ontology: &'a OntologyIR,
}

impl<'a> InterfaceExpander<'a> {
    pub fn new(ontology: &'a OntologyIR) -> Self {
        Self { ontology }
    }

    /// Every node type whose `implements` contains `interface_id`.
    ///
    /// Fails when the interface is not declared on the ontology —
    /// `(:Ghost)` where `Ghost` is not a known interface *and* not a
    /// known node type would reach this code path only via a planner
    /// bug, so an explicit rejection is better than an empty success.
    pub fn expand(&self, interface_id: &InterfaceId) -> FederationResult<ExpandedTargets> {
        let interface_exists = self
            .ontology
            .interfaces()
            .iter()
            .any(|i| &i.id == interface_id);
        if !interface_exists {
            return Err(FederationError::unsupported(format!(
                "InterfaceExpander: interface '{interface_id}' is not declared on the ontology"
            )));
        }

        let node_type_ids: Vec<NodeTypeId> = self
            .ontology
            .node_types()
            .iter()
            .filter(|n| n.implements.iter().any(|id| id == interface_id))
            .map(|n| n.id.clone())
            .collect();

        Ok(ExpandedTargets {
            interface_id: interface_id.clone(),
            node_type_ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::interface::InterfaceDef;
    use ox_ontology::ir::NodeTypeDef;

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("valid")
    }

    fn gl_dynamic(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("valid")
    }

    fn interface(id: &str, label: &'static str) -> InterfaceDef {
        InterfaceDef {
            id: InterfaceId::new(id),
            label: gl(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            required_properties: vec![],
            required_edges: vec![],
        }
    }

    fn node_with_interfaces(id: &str, label: &str, ifaces: Vec<&str>) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: gl_dynamic(label),
            implements: ifaces.into_iter().map(InterfaceId::new).collect(),
            ..Default::default()
        }
    }

    fn ontology_with(nodes: Vec<NodeTypeDef>, interfaces: Vec<InterfaceDef>) -> OntologyIR {
        let mut o = OntologyIR::new(
            "ont".into(),
            "test".into(),
            LocalizedText::default(),
            1,
            nodes,
            vec![],
            vec![],
        );
        for i in interfaces {
            o.add_interface(i).unwrap();
        }
        o
    }

    #[test]
    fn unknown_interface_surfaces_as_unsupported() {
        let ont = ontology_with(vec![], vec![interface("if-1", "HasAddress")]);
        let ex = InterfaceExpander::new(&ont);
        let err = ex
            .expand(&InterfaceId::new("if-ghost"))
            .expect_err("must reject");
        assert!(matches!(err, FederationError::Unsupported(_)));
    }

    #[test]
    fn known_interface_with_no_implementers_returns_empty_targets() {
        let ont = ontology_with(vec![], vec![interface("if-1", "HasAddress")]);
        let ex = InterfaceExpander::new(&ont);
        let out = ex.expand(&InterfaceId::new("if-1")).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn expansion_collects_every_implementer_in_declaration_order() {
        let ont = ontology_with(
            vec![
                node_with_interfaces("nt-user", "User", vec!["if-1"]),
                node_with_interfaces("nt-biz", "Business", vec![]),
                node_with_interfaces("nt-site", "Site", vec!["if-1", "if-other"]),
            ],
            vec![
                interface("if-1", "HasAddress"),
                interface("if-other", "Geolocated"),
            ],
        );
        let ex = InterfaceExpander::new(&ont);
        let out = ex.expand(&InterfaceId::new("if-1")).unwrap();
        let ids: Vec<&str> = out.node_type_ids.iter().map(|n| n.as_str()).collect();
        assert_eq!(ids, vec!["nt-user", "nt-site"]);
    }

    #[test]
    fn different_interfaces_expand_disjointly() {
        let ont = ontology_with(
            vec![
                node_with_interfaces("nt-a", "A", vec!["if-1"]),
                node_with_interfaces("nt-b", "B", vec!["if-2"]),
            ],
            vec![interface("if-1", "One"), interface("if-2", "Two")],
        );
        let ex = InterfaceExpander::new(&ont);
        assert_eq!(
            ex.expand(&InterfaceId::new("if-1")).unwrap().node_type_ids,
            vec![NodeTypeId::new("nt-a")]
        );
        assert_eq!(
            ex.expand(&InterfaceId::new("if-2")).unwrap().node_type_ids,
            vec![NodeTypeId::new("nt-b")]
        );
    }
}

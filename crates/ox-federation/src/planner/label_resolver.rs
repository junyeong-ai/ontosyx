//! `LabelResolver` — disambiguate a `GraphLabel` reference in a
//! query.
//!
//! A `GraphLabel` in a query (`MATCH (n:HasAddress)`, `MATCH (c:Customer)`)
//! names either a concrete node type or an interface — the planner
//! cannot tell from the text alone. Which side the label lives on
//! determines whether the downstream stages expand the target
//! (interface → union of implementers) or resolve mappings directly
//! (concrete type → object mappings).
//!
//! `LabelResolver` is the thin entry point that the rest of the
//! pipeline calls to answer that question. It composes over the
//! existing ontology accessors and over [`InterfaceExpander`] — no
//! new state, no I/O.
//!
//! # Ambiguity
//!
//! An ontology that declares both a `NodeTypeDef` with label
//! `Customer` and an `InterfaceDef` also labelled `Customer` is
//! self-contradictory. The ontology validator already rejects
//! duplicate node-type labels at construction; interfaces live in a
//! separate collection, so a collision across the two is possible
//! (and exactly the kind of surprise we want to surface loudly).
//! `LabelResolver` reports `Ambiguous` in that case instead of
//! silently picking one — the planner refuses to compile and the
//! ontology editor gets a clear diagnostic.

use ox_core::graph_label::GraphLabel;
use ox_ontology::OntologyIR;
use ox_ontology::interface::InterfaceId;
use ox_ontology::ir::NodeTypeId;

use crate::error::{FederationError, FederationResult};
use crate::planner::interface_expander::InterfaceExpander;

/// Outcome of resolving one label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedLabelTarget {
    /// The label names a concrete node type. The planner resolves
    /// mappings directly from `MappingResolver::resolve_node_type`.
    Concrete(NodeTypeId),

    /// The label names an interface. `implementers` enumerates every
    /// node type whose `implements` contains the interface id —
    /// expansion has already run, so the planner can iterate without
    /// a second `InterfaceExpander` call.
    Interface {
        interface_id: InterfaceId,
        implementers: Vec<NodeTypeId>,
    },

    /// The label matches both a node type and an interface — the
    /// ontology is self-contradictory. The planner refuses to
    /// compile and returns this variant so the UI / editor can
    /// point at the collision.
    Ambiguous {
        node_type_id: NodeTypeId,
        interface_id: InterfaceId,
    },
}

impl ResolvedLabelTarget {
    /// Enumerate every node type the target resolves to. Returns one
    /// id for `Concrete`, every implementer for `Interface`, or the
    /// single colliding node-type id for `Ambiguous` (the caller is
    /// responsible for rejecting `Ambiguous` upstream before
    /// iterating — this helper is a convenience for tests and
    /// diagnostics).
    pub fn node_type_ids(&self) -> Vec<&NodeTypeId> {
        match self {
            ResolvedLabelTarget::Concrete(id) => vec![id],
            ResolvedLabelTarget::Interface { implementers, .. } => implementers.iter().collect(),
            ResolvedLabelTarget::Ambiguous { node_type_id, .. } => vec![node_type_id],
        }
    }

    /// Is this the ambiguous variant? The planner checks this
    /// explicitly and refuses to emit a plan when true — same
    /// posture as SHACL's `sh:Violation`.
    pub fn is_ambiguous(&self) -> bool {
        matches!(self, ResolvedLabelTarget::Ambiguous { .. })
    }
}

/// Pure-function resolver — constructed once per query plan,
/// borrowed read-only across every label resolution.
#[derive(Debug, Clone)]
pub struct LabelResolver<'a> {
    ontology: &'a OntologyIR,
}

impl<'a> LabelResolver<'a> {
    pub fn new(ontology: &'a OntologyIR) -> Self {
        Self { ontology }
    }

    /// Resolve a `GraphLabel` to a [`ResolvedLabelTarget`].
    ///
    /// Fails only when the label matches nothing at all. The
    /// `Ambiguous` variant is a *successful* resolution that simply
    /// reports a malformed ontology — distinguishing the two lets
    /// the planner treat "user typed a label we don't know" and
    /// "this ontology is self-contradictory" as separate error
    /// classes in its diagnostics.
    pub fn resolve(&self, label: &GraphLabel) -> FederationResult<ResolvedLabelTarget> {
        let as_node: Option<NodeTypeId> = self
            .ontology
            .node_types()
            .iter()
            .find(|n| &n.label == label)
            .map(|n| n.id.clone());

        let as_interface: Option<InterfaceId> = self
            .ontology
            .interfaces()
            .iter()
            .find(|i| &i.label == label)
            .map(|i| i.id.clone());

        match (as_node, as_interface) {
            (Some(node_type_id), Some(interface_id)) => Ok(ResolvedLabelTarget::Ambiguous {
                node_type_id,
                interface_id,
            }),
            (Some(node_type_id), None) => Ok(ResolvedLabelTarget::Concrete(node_type_id)),
            (None, Some(interface_id)) => {
                // `InterfaceExpander::expand` enforces that the
                // interface is declared on the ontology (which we
                // just verified) so the expansion cannot fail — but
                // we still propagate its `FederationResult` to keep
                // call-sites honest if that contract changes.
                let expansion =
                    InterfaceExpander::new(self.ontology).expand(&interface_id)?;
                Ok(ResolvedLabelTarget::Interface {
                    interface_id,
                    implementers: expansion.node_type_ids,
                })
            }
            (None, None) => Err(FederationError::unsupported(format!(
                "LabelResolver: label '{label}' is not a node type or interface in the ontology"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::interface::InterfaceDef;
    use ox_ontology::ir::NodeTypeDef;

    fn gl(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("valid label")
    }

    fn node(id: &str, label: &str, implements: Vec<&str>) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: gl(label),
            implements: implements.into_iter().map(InterfaceId::new).collect(),
            ..Default::default()
        }
    }

    fn interface(id: &str, label: &str) -> InterfaceDef {
        InterfaceDef {
            id: InterfaceId::new(id),
            label: gl(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            required_properties: vec![],
            required_edges: vec![],
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
    fn concrete_node_type_label_resolves_to_concrete_variant() {
        let ont = ontology_with(vec![node("nt-user", "User", vec![])], vec![]);
        let r = LabelResolver::new(&ont);
        let target = r.resolve(&gl("User")).unwrap();
        assert_eq!(target, ResolvedLabelTarget::Concrete(NodeTypeId::new("nt-user")));
        assert!(!target.is_ambiguous());
    }

    #[test]
    fn interface_label_resolves_with_expanded_implementers() {
        let ont = ontology_with(
            vec![
                node("nt-user", "User", vec!["if-addr"]),
                node("nt-org", "Org", vec!["if-addr"]),
                node("nt-tag", "Tag", vec![]),
            ],
            vec![interface("if-addr", "HasAddress")],
        );
        let r = LabelResolver::new(&ont);
        let target = r.resolve(&gl("HasAddress")).unwrap();
        match target {
            ResolvedLabelTarget::Interface {
                interface_id,
                implementers,
            } => {
                assert_eq!(interface_id, InterfaceId::new("if-addr"));
                let ids: Vec<&str> = implementers.iter().map(|n| n.as_str()).collect();
                assert_eq!(ids, vec!["nt-user", "nt-org"]);
            }
            other => panic!("expected Interface, got {other:?}"),
        }
    }

    #[test]
    fn collision_between_node_type_and_interface_surfaces_ambiguous() {
        let ont = ontology_with(
            vec![node("nt-customer", "Customer", vec![])],
            // Interface author re-used the label (ontology bug) —
            // resolver must flag rather than silently pick.
            vec![interface("if-customer", "Customer")],
        );
        let r = LabelResolver::new(&ont);
        let target = r.resolve(&gl("Customer")).unwrap();
        assert!(target.is_ambiguous());
        match target {
            ResolvedLabelTarget::Ambiguous {
                node_type_id,
                interface_id,
            } => {
                assert_eq!(node_type_id, NodeTypeId::new("nt-customer"));
                assert_eq!(interface_id, InterfaceId::new("if-customer"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn unknown_label_surfaces_as_unsupported() {
        let ont = ontology_with(vec![node("nt-user", "User", vec![])], vec![]);
        let r = LabelResolver::new(&ont);
        let err = r.resolve(&gl("Ghost")).expect_err("must reject");
        assert!(matches!(err, FederationError::Unsupported(_)));
    }

    #[test]
    fn node_type_ids_helper_enumerates_every_target_variant() {
        // Concrete → 1 id.
        let concrete = ResolvedLabelTarget::Concrete(NodeTypeId::new("nt-1"));
        assert_eq!(concrete.node_type_ids().len(), 1);

        // Interface → every implementer.
        let iface = ResolvedLabelTarget::Interface {
            interface_id: InterfaceId::new("if-1"),
            implementers: vec![NodeTypeId::new("nt-a"), NodeTypeId::new("nt-b")],
        };
        assert_eq!(iface.node_type_ids().len(), 2);

        // Ambiguous → single colliding node-type id. The planner
        // rejects Ambiguous before iterating; this path exists for
        // diagnostics only.
        let amb = ResolvedLabelTarget::Ambiguous {
            node_type_id: NodeTypeId::new("nt-c"),
            interface_id: InterfaceId::new("if-c"),
        };
        assert_eq!(amb.node_type_ids().len(), 1);
    }
}

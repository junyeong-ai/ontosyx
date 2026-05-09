//! `MappingResolver` — resolves a node type to the physical mappings
//! the federation engine can scan for it.
//!
//! For a given `(NodeTypeId, optional ontology_valid_at)` pair the
//! resolver walks `OntologyIR::object_mappings()` and produces a list
//! of applicable mappings, sorted by `precedence` descending so the
//! caller can deduplicate multi-mapping results with a simple
//! "first-match-wins" policy.
//!
//! The resolver is read-only and synchronous — all data is already in
//! the `OntologyIR` snapshot.

use chrono::{DateTime, Utc};

use ox_ontology::OntologyIR;
use ox_ontology::ir::EdgeTypeId;
use ox_ontology::ir::NodeTypeId;
use ox_ontology::mapping::{LinkMappingDef, ObjectMappingDef};

use crate::error::{FederationError, FederationResult};

/// Output of [`MappingResolver::resolve_node_type`].
///
/// Ordering: highest `precedence` first. Same-`precedence` mappings
/// keep their source-order for determinism (useful for tests and for
/// stable diagnostics).
#[derive(Debug, Clone)]
pub struct ResolvedMappings<'a> {
    pub node_type_id: NodeTypeId,
    /// Non-empty when the resolver succeeds. Returned as `&ObjectMappingDef`
    /// so the caller does not allocate; the planner typically
    /// re-borrows each entry to feed a `TableProvider` registration.
    pub mappings: Vec<&'a ObjectMappingDef>,
    /// Whether any mapping was filtered out because of the
    /// temporal-window check. `true` → the resolver *saw* mappings
    /// for this node type but none were valid at the requested time.
    /// Distinguishes "ontology does not map this type" from
    /// "ontology does but not at this timestamp".
    pub filtered_by_temporal: bool,
}

impl<'a> ResolvedMappings<'a> {
    /// Convenience — is the resolved set empty?
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    /// Top-precedence mapping, or `None` when the set is empty.
    pub fn primary(&self) -> Option<&'a ObjectMappingDef> {
        self.mappings.first().copied()
    }
}

/// Pure-function resolver over an `OntologyIR` snapshot.
#[derive(Debug, Clone)]
pub struct MappingResolver<'a> {
    ontology: &'a OntologyIR,
    at: Option<DateTime<Utc>>,
}

impl<'a> MappingResolver<'a> {
    /// Resolver that accepts mappings with *any* temporal window —
    /// suited for read-time paths where the caller only cares about
    /// the structurally-applicable mappings.
    pub fn new(ontology: &'a OntologyIR) -> Self {
        Self { ontology, at: None }
    }

    /// Resolver pinned to `at`. Only mappings whose `valid_from`/
    /// `valid_to` contain `at` pass through. Used by the bitemporal
    /// query path so a query with `ontology_valid_at = 2025-01-01`
    /// sees the mapping world as it was on that date, not today.
    pub fn at(ontology: &'a OntologyIR, at: DateTime<Utc>) -> Self {
        Self {
            ontology,
            at: Some(at),
        }
    }

    /// Mappings that apply to `node_type_id`, sorted by `precedence`
    /// descending.
    ///
    /// Returns `Err(FederationError::Unsupported)` when the node type
    /// is not declared on the ontology — the caller is referencing a
    /// concept the ontology does not know about, which is a planner
    /// logic bug rather than a data-level mapping miss.
    pub fn resolve_node_type(
        &self,
        node_type_id: &NodeTypeId,
    ) -> FederationResult<ResolvedMappings<'a>> {
        if self.ontology.node_by_id(node_type_id.as_str()).is_none() {
            return Err(FederationError::unsupported(format!(
                "MappingResolver: node type '{node_type_id}' is not declared on the ontology"
            )));
        }

        let mut considered = 0usize;
        let mut mappings: Vec<&ObjectMappingDef> = self
            .ontology
            .object_mappings()
            .iter()
            .filter(|m| &m.node_type_id == node_type_id)
            .inspect(|_| {
                considered += 1;
            })
            .filter(|m| match self.at {
                Some(t) => m.is_valid_at(t),
                None => true,
            })
            .collect();

        // Highest precedence first; ties resolve by mapping id ascending
        // so the order is independent of the IR's insertion sequence.
        mappings.sort_by(|a, b| {
            b.precedence
                .cmp(&a.precedence)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });

        Ok(ResolvedMappings {
            node_type_id: node_type_id.clone(),
            filtered_by_temporal: mappings.is_empty() && considered > 0,
            mappings,
        })
    }

    /// Link mappings that apply to `edge_type_id`. Same precedence
    /// ordering semantics as [`Self::resolve_node_type`].
    pub fn resolve_edge_type(
        &self,
        edge_type_id: &EdgeTypeId,
    ) -> FederationResult<Vec<&'a LinkMappingDef>> {
        // Edge ids aren't surfaced through an `edge_by_id` lookup on
        // OntologyIR — iterate once instead. `OntologyIR::edge_types`
        // is small (~1000 edges max) so this stays cheap.
        let declared = self
            .ontology
            .edge_types()
            .iter()
            .any(|e| &e.id == edge_type_id);
        if !declared {
            return Err(FederationError::unsupported(format!(
                "MappingResolver: edge type '{edge_type_id}' is not declared on the ontology"
            )));
        }

        let mut mappings: Vec<&LinkMappingDef> = self
            .ontology
            .link_mappings()
            .iter()
            .filter(|m| &m.edge_type_id == edge_type_id)
            .collect();

        mappings.sort_by(|a, b| {
            b.precedence
                .cmp(&a.precedence)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        Ok(mappings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::ir::NodeTypeDef;
    use ox_ontology::mapping::ObjectMappingDef;

    fn ontology_with(nodes: Vec<NodeTypeDef>) -> OntologyIR {
        OntologyIR::new(
            "ont".into(),
            "test".into(),
            LocalizedText::default(),
            1,
            nodes,
            vec![],
            vec![],
        )
    }

    fn node(id: &str, label: &str) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: gl_dynamic(label),
            ..Default::default()
        }
    }

    fn gl_dynamic(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("valid")
    }

    fn with_precedence(
        id: &str,
        node_type: &str,
        source: &str,
        relation: &str,
        precedence: u32,
    ) -> ObjectMappingDef {
        let mut m = ObjectMappingDef::new(id, node_type, source, relation);
        m.precedence = precedence;
        m
    }

    #[test]
    fn unknown_node_type_surfaces_as_unsupported() {
        let ont = ontology_with(vec![node("nt-1", "User")]);
        let r = MappingResolver::new(&ont);
        let err = r
            .resolve_node_type(&NodeTypeId::new("nt-ghost"))
            .expect_err("must reject");
        assert!(matches!(err, FederationError::Unsupported(_)));
    }

    #[test]
    fn empty_mapping_set_returns_empty_resolved() {
        let ont = ontology_with(vec![node("nt-1", "User")]);
        let r = MappingResolver::new(&ont);
        let out = r.resolve_node_type(&NodeTypeId::new("nt-1")).unwrap();
        assert!(out.is_empty());
        assert!(!out.filtered_by_temporal);
        assert!(out.primary().is_none());
    }

    #[test]
    fn multiple_mappings_sort_by_precedence_desc_stable_within_ties() {
        let mut ont = ontology_with(vec![node("nt-1", "User")]);
        ont.add_object_mapping(with_precedence("om-low", "nt-1", "pg", "users_v1", 10))
            .unwrap();
        ont.add_object_mapping(with_precedence("om-high", "nt-1", "pg", "users_v3", 200))
            .unwrap();
        ont.add_object_mapping(with_precedence("om-mid-a", "nt-1", "pg", "users_v2_a", 100))
            .unwrap();
        ont.add_object_mapping(with_precedence("om-mid-b", "nt-1", "pg", "users_v2_b", 100))
            .unwrap();

        let r = MappingResolver::new(&ont);
        let out = r.resolve_node_type(&NodeTypeId::new("nt-1")).unwrap();
        let ids: Vec<&str> = out.mappings.iter().map(|m| m.id.as_str()).collect();
        // highest first, then stable original order for the two 100s.
        assert_eq!(ids, vec!["om-high", "om-mid-a", "om-mid-b", "om-low"]);
    }

    #[test]
    fn at_filters_out_mappings_outside_their_window_and_reports_temporal_miss() {
        let mut ont = ontology_with(vec![node("nt-1", "User")]);

        let mut old = with_precedence("om-old", "nt-1", "pg", "users", 100);
        old.valid_to = Some(Utc::now() - chrono::Duration::hours(1));

        let mut future = with_precedence("om-future", "nt-1", "pg", "users_new", 100);
        future.valid_from = Some(Utc::now() + chrono::Duration::hours(1));

        ont.add_object_mapping(old).unwrap();
        ont.add_object_mapping(future).unwrap();

        let r = MappingResolver::at(&ont, Utc::now());
        let out = r.resolve_node_type(&NodeTypeId::new("nt-1")).unwrap();
        assert!(out.is_empty());
        assert!(
            out.filtered_by_temporal,
            "both mappings existed but fell outside the window — resolver must signal that"
        );
    }

    #[test]
    fn no_temporal_filter_accepts_every_matching_mapping() {
        let mut ont = ontology_with(vec![node("nt-1", "User")]);
        let mut m = with_precedence("om-archive", "nt-1", "pg", "archive", 100);
        m.valid_to = Some(Utc::now() - chrono::Duration::days(365));
        ont.add_object_mapping(m).unwrap();

        let r = MappingResolver::new(&ont);
        let out = r.resolve_node_type(&NodeTypeId::new("nt-1")).unwrap();
        assert_eq!(out.mappings.len(), 1);
        assert!(!out.filtered_by_temporal);
    }
}

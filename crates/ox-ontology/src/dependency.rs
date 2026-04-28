//! Schema-level dependency graph derived from [`OntologyIR`].
//!
//! Distinct from PROV-O / [`crate::ProvenanceDef`] which tracks
//! **instance-level** lineage (which row came from which source);
//! this module models **schema-level** dependencies between
//! definition entities (which `RuleDef` constrains which
//! `PropertyDef`, which `ObjectMappingDef` targets which
//! `NodeTypeDef`, etc.).
//!
//! Used to answer the impact-analysis question: *"if I change this
//! entity, what else breaks?"* — surfaced in the editor's Inspector
//! and the standalone `/dependencies` view.
//!
//! ## Construction
//!
//! [`SchemaDependencyGraph::build`] walks every reference in the IR
//! exactly once, producing an inverted index (target →
//! [`DependencyEdge`] list). The walk uses the IR's existing
//! `lookup` indices for O(1) presence checks; the build cost is
//! linear in the total reference count.
//!
//! ## Query
//!
//! [`SchemaDependencyGraph::dependents_of`] returns the
//! [`DependencyEdge`] slice for any [`SchemaEntityRef`]. The slice is
//! sorted deterministically by `(kind, dependent)` so consecutive
//! calls produce stable output for snapshot tests and FE diffs.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::binding::PropertyBinding;
use crate::ir::OntologyIR;
use crate::mapping::LinkMappingKind;
use crate::rule::{ConstraintTarget, RuleActivationKind, RuleKind, ShaclConstraint};

// ---------------------------------------------------------------------------
// DependencyBucket — wire-friendly tuple
// ---------------------------------------------------------------------------

/// One entry in [`SchemaDependencyGraph::buckets`] — a target and
/// every inbound dependency. Tuple-shaped on the wire so the
/// graph round-trips through JSON cleanly (enum keys can't ride
/// in JSON object keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct DependencyBucket {
    pub target: SchemaEntityRef,
    pub edges: Vec<DependencyEdge>,
}

// ---------------------------------------------------------------------------
// SchemaEntityRef — addressable handle for any IR entity
// ---------------------------------------------------------------------------

/// Stable handle that addresses any first-class entity in an
/// [`OntologyIR`]. Used as the key into
/// [`SchemaDependencyGraph`]'s inverted index.
///
/// Each variant carries plain `String` ids (not the typed
/// `XxxId` newtypes) so the graph is self-contained — callers can
/// serialise the whole shape over the wire without depending on
/// the per-id wrapper types.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
    utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaEntityRef {
    NodeType { id: String },
    EdgeType { id: String },
    /// `owner` is the parent NodeType or EdgeType id; `id` is the
    /// PropertyId. Properties are nested entities — the owner
    /// disambiguates same-named properties on different parents.
    Property { owner: String, id: String },
    Interface { id: String },
    GlossaryTerm { id: String },
    ValueSet { id: String },
    CodeSystem { id: String },
    NotationPattern { id: String },
    ValueRangeSet { id: String },
    Rule { id: String },
    Function { id: String },
    Metric { id: String },
    Action { id: String },
    Enrichment { id: String },
    ObjectMapping { id: String },
    LinkMapping { id: String },
    ConceptMap { id: String },
    DataQuality { id: String },
    /// CodedValue lives inside a CodeSystem; pinned by both ids
    /// so a property's `unit_id` can deep-link to the correct
    /// system.
    CodedValue { code_system: String, id: String },
}

// ---------------------------------------------------------------------------
// DependencyEdge — directional reference
// ---------------------------------------------------------------------------

/// A single reverse-edge in the dependency graph: the
/// [`SchemaEntityRef`] target was referenced by `dependent` via a
/// [`DependencyKind`] relationship.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
    utoipa::ToSchema,
)]
pub struct DependencyEdge {
    pub dependent: SchemaEntityRef,
    pub kind: DependencyKind,
    /// Short human-readable summary of *how* the dependent
    /// references the target — surfaced as a tooltip in the FE
    /// (`"MinCount constraint"`, `"foreign-key bridge"`, etc.).
    pub label: String,
}

/// Classification of a [`DependencyEdge`]. Each kind carries a
/// distinct semantic — the FE picks an icon and groups by kind.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    /// Property belongs to a NodeType / EdgeType.
    PropertyOf,
    /// EdgeType references a NodeType as its source endpoint.
    EdgeSource,
    /// EdgeType references a NodeType as its target endpoint.
    EdgeTarget,
    /// NodeType lists an Interface in its `implements` set.
    InterfaceImplementation,
    /// Property carries a [`PropertyBinding`] to the target
    /// (ValueSet / CodeSystem / NotationPattern / ValueRangeSet /
    /// GlossaryTerm).
    PropertyBindingRef,
    /// Property's `derived_from` references a Function.
    FunctionDerivation,
    /// Property's `unit_id` references a CodedValue.
    UnitReference,
    /// Rule constrains the target (NodeType / EdgeType / Property)
    /// via [`RuleKind`] / [`ConstraintTarget`].
    RuleConstraint,
    /// Rule's [`RuleActivationKind::OnAction`] references an Action.
    RuleActivation,
    /// Rule's [`ShaclConstraint`] references a ValueSet (`InValueSet`)
    /// or NotationPattern (`MatchesPattern`).
    RuleVocabulary,
    /// Metric's scope references a NodeType or EdgeType.
    MetricScope,
    /// Action's target references a NodeType or EdgeType.
    ActionTarget,
    /// Action's pre/post-condition references a Rule.
    ActionRule,
    /// Enrichment targets a NodeType.
    EnrichmentTarget,
    /// ObjectMapping targets a NodeType.
    ObjectMappingTarget,
    /// LinkMapping targets an EdgeType.
    LinkMappingTarget,
    /// PropertyMapping targets a Property.
    PropertyMappingTarget,
    /// ValueSet's composition rule references a CodeSystem.
    ValueSetComposition,
    /// ConceptMap's source or target references a CodeSystem.
    ConceptMapEndpoint,
    /// DataQuality targets a NodeType or Property.
    DataQualityTarget,
}

// ---------------------------------------------------------------------------
// SchemaDependencyGraph
// ---------------------------------------------------------------------------

/// Inverted index of every schema-level reference in an
/// [`OntologyIR`]. Build once per snapshot; query many times via
/// [`SchemaDependencyGraph::dependents_of`].
///
/// Stored as a sorted [`DependencyBucket`] vector so the wire
/// shape is a clean JSON array and `dependents_of` runs in
/// O(log n) via binary search. Both build and query paths are
/// deterministic.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct SchemaDependencyGraph {
    /// Targets sorted by [`SchemaEntityRef`] ordering; each
    /// bucket's edges sorted within.
    pub buckets: Vec<DependencyBucket>,
}

impl SchemaDependencyGraph {
    /// Walk every reference in `ontology` and produce the inverted
    /// index. Linear in total reference count.
    pub fn build(ontology: &OntologyIR) -> Self {
        let mut edges: BTreeMap<SchemaEntityRef, Vec<DependencyEdge>> = BTreeMap::new();

        // ---- NodeTypes ------------------------------------------------
        for node in ontology.node_types() {
            let node_ref = SchemaEntityRef::NodeType { id: node.id.as_str().to_string() };

            // Properties on a node depend on it via PropertyOf.
            for prop in &node.properties {
                add_edge(
                    &mut edges,
                    node_ref.clone(),
                    SchemaEntityRef::Property {
                        owner: node.id.as_str().to_string(),
                        id: prop.id.as_str().to_string(),
                    },
                    DependencyKind::PropertyOf,
                    format!("property `{}`", prop.name),
                );
                walk_property(&mut edges, &node.id, prop);
            }

            // Interface implementations.
            for if_id in &node.implements {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Interface { id: if_id.as_str().to_string() },
                    node_ref.clone(),
                    DependencyKind::InterfaceImplementation,
                    format!("implemented by `{}`", node.label),
                );
            }

            // Direct rule attachments on the node — captured as
            // RuleConstraint. The rule's own kind/constraints are
            // walked separately (below) so `dependents_of` on a
            // property-shape rule's target sees both this attach
            // edge and the property-shape edge.
            for rule_id in &node.rules {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Rule { id: rule_id.as_str().to_string() },
                    node_ref.clone(),
                    DependencyKind::RuleConstraint,
                    format!("attached to `{}`", node.label),
                );
            }

            for action_id in &node.actions {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Action { id: action_id.as_str().to_string() },
                    node_ref.clone(),
                    DependencyKind::ActionTarget,
                    format!("attached to `{}`", node.label),
                );
            }

            for metric_id in &node.metrics {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Metric { id: metric_id.as_str().to_string() },
                    node_ref.clone(),
                    DependencyKind::MetricScope,
                    format!("scoped to `{}`", node.label),
                );
            }
        }

        // ---- EdgeTypes ------------------------------------------------
        for edge in ontology.edge_types() {
            let edge_ref = SchemaEntityRef::EdgeType { id: edge.id.as_str().to_string() };

            add_edge(
                &mut edges,
                SchemaEntityRef::NodeType { id: edge.source_node_id.as_str().to_string() },
                edge_ref.clone(),
                DependencyKind::EdgeSource,
                format!("source of `{}`", edge.label),
            );
            add_edge(
                &mut edges,
                SchemaEntityRef::NodeType { id: edge.target_node_id.as_str().to_string() },
                edge_ref.clone(),
                DependencyKind::EdgeTarget,
                format!("target of `{}`", edge.label),
            );

            for prop in &edge.properties {
                add_edge(
                    &mut edges,
                    edge_ref.clone(),
                    SchemaEntityRef::Property {
                        owner: edge.id.as_str().to_string(),
                        id: prop.id.as_str().to_string(),
                    },
                    DependencyKind::PropertyOf,
                    format!("property `{}`", prop.name),
                );
                walk_property(&mut edges, &edge.id, prop);
            }
        }

        // ---- Rules ----------------------------------------------------
        for rule in &ontology.rules {
            let rule_ref = SchemaEntityRef::Rule { id: rule.id.as_str().to_string() };

            // Target references — derived from `RuleKind`.
            match &rule.kind {
                RuleKind::NodeShape { target_node_type_id } => {
                    add_edge(
                        &mut edges,
                        SchemaEntityRef::NodeType { id: target_node_type_id.as_str().to_string() },
                        rule_ref.clone(),
                        DependencyKind::RuleConstraint,
                        format!("rule `{}` (NodeShape)", rule.id),
                    );
                }
                RuleKind::PropertyShape {
                    target_node_type_id,
                    target_property_id,
                } => {
                    add_edge(
                        &mut edges,
                        SchemaEntityRef::Property {
                            owner: target_node_type_id.as_str().to_string(),
                            id: target_property_id.as_str().to_string(),
                        },
                        rule_ref.clone(),
                        DependencyKind::RuleConstraint,
                        format!("rule `{}` (PropertyShape)", rule.id),
                    );
                }
                RuleKind::EdgeShape { target_edge_type_id } => {
                    add_edge(
                        &mut edges,
                        SchemaEntityRef::EdgeType { id: target_edge_type_id.as_str().to_string() },
                        rule_ref.clone(),
                        DependencyKind::RuleConstraint,
                        format!("rule `{}` (EdgeShape)", rule.id),
                    );
                }
                RuleKind::CrossEntityShape { .. } | RuleKind::StateMachine { .. } => {
                    // CrossEntityShape carries source-dialect SQL —
                    // dependency targets are opaque. StateMachine's
                    // node + property are not exposed as separate
                    // dependency edges in this MVP.
                }
            }

            // Constraint vocabulary references.
            for constraint in &rule.constraints {
                walk_constraint(&mut edges, &rule_ref, &rule.id, constraint);
            }

            // OnAction activation.
            if let RuleActivationKind::OnAction { action_id } = &rule.activation {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Action { id: action_id.as_str().to_string() },
                    rule_ref.clone(),
                    DependencyKind::RuleActivation,
                    format!("rule `{}` activates on action", rule.id),
                );
            }
        }

        // ---- Mappings -------------------------------------------------
        for om in ontology.object_mappings() {
            add_edge(
                &mut edges,
                SchemaEntityRef::NodeType { id: om.node_type_id.as_str().to_string() },
                SchemaEntityRef::ObjectMapping { id: om.id.as_str().to_string() },
                DependencyKind::ObjectMappingTarget,
                format!("mapped to relation `{}`", om.relation),
            );
            for pm in &om.property_mappings {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Property {
                        owner: om.node_type_id.as_str().to_string(),
                        id: pm.property_id.as_str().to_string(),
                    },
                    SchemaEntityRef::ObjectMapping { id: om.id.as_str().to_string() },
                    DependencyKind::PropertyMappingTarget,
                    "object-mapping property binding".to_string(),
                );
            }
        }
        for lm in ontology.link_mappings() {
            add_edge(
                &mut edges,
                SchemaEntityRef::EdgeType { id: lm.edge_type_id.as_str().to_string() },
                SchemaEntityRef::LinkMapping { id: lm.id.as_str().to_string() },
                DependencyKind::LinkMappingTarget,
                link_mapping_label(&lm.kind),
            );
        }

        // ---- ValueSets references CodeSystems ------------------------
        for vs in ontology.value_sets() {
            for (rule_index, rule) in vs.composition.iter().enumerate() {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::CodeSystem {
                        id: rule.system_id.as_str().to_string(),
                    },
                    SchemaEntityRef::ValueSet { id: vs.id.as_str().to_string() },
                    DependencyKind::ValueSetComposition,
                    format!(
                        "value set `{}` composition rule {rule_index}",
                        vs.name
                    ),
                );
            }
        }

        // ---- ConceptMap endpoints ------------------------------------
        for cm in ontology.concept_maps() {
            add_edge(
                &mut edges,
                SchemaEntityRef::CodeSystem { id: cm.source_system_id.as_str().to_string() },
                SchemaEntityRef::ConceptMap { id: cm.id.as_str().to_string() },
                DependencyKind::ConceptMapEndpoint,
                format!("concept map `{}` source", cm.name),
            );
            add_edge(
                &mut edges,
                SchemaEntityRef::CodeSystem { id: cm.target_system_id.as_str().to_string() },
                SchemaEntityRef::ConceptMap { id: cm.id.as_str().to_string() },
                DependencyKind::ConceptMapEndpoint,
                format!("concept map `{}` target", cm.name),
            );
        }

        // ---- Enrichments ---------------------------------------------
        for enr in ontology.enrichments() {
            add_edge(
                &mut edges,
                SchemaEntityRef::NodeType { id: enr.target_node_type_id.as_str().to_string() },
                SchemaEntityRef::Enrichment { id: enr.id.as_str().to_string() },
                DependencyKind::EnrichmentTarget,
                format!("enrichment `{}`", enr.name),
            );
        }

        // ---- Actions: pre/post-condition rules -----------------------
        for action in ontology.actions() {
            for rule_id in action.preconditions.iter().chain(action.postconditions.iter()) {
                add_edge(
                    &mut edges,
                    SchemaEntityRef::Rule { id: rule_id.as_str().to_string() },
                    SchemaEntityRef::Action { id: action.id.as_str().to_string() },
                    DependencyKind::ActionRule,
                    format!("action `{}` references rule", action.name),
                );
            }
        }

        // ---- DataQuality targets -------------------------------------
        for dq in ontology.data_quality() {
            for target_ref in data_quality_targets(dq) {
                add_edge(
                    &mut edges,
                    target_ref,
                    SchemaEntityRef::DataQuality { id: dq.id.as_str().to_string() },
                    DependencyKind::DataQualityTarget,
                    format!("data quality `{}`", dq.id),
                );
            }
        }

        // Sort each bucket for deterministic output, then collapse
        // the BTreeMap into a sorted Vec for wire-stable shape.
        for bucket in edges.values_mut() {
            bucket.sort();
        }
        let buckets = edges
            .into_iter()
            .map(|(target, edges)| DependencyBucket { target, edges })
            .collect();

        Self { buckets }
    }

    /// Return the inbound [`DependencyEdge`]s for `target`. Empty
    /// slice when no entity references the target. O(log n) via
    /// binary search on the sorted bucket vector.
    pub fn dependents_of(&self, target: &SchemaEntityRef) -> &[DependencyEdge] {
        match self
            .buckets
            .binary_search_by(|b| b.target.cmp(target))
        {
            Ok(idx) => &self.buckets[idx].edges,
            Err(_) => &[],
        }
    }

    /// Total number of recorded edges across every target.
    pub fn edge_count(&self) -> usize {
        self.buckets.iter().map(|b| b.edges.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn add_edge(
    edges: &mut BTreeMap<SchemaEntityRef, Vec<DependencyEdge>>,
    target: SchemaEntityRef,
    dependent: SchemaEntityRef,
    kind: DependencyKind,
    label: String,
) {
    edges.entry(target).or_default().push(DependencyEdge {
        dependent,
        kind,
        label,
    });
}

fn walk_property(
    edges: &mut BTreeMap<SchemaEntityRef, Vec<DependencyEdge>>,
    owner_id: &impl AsRef<str>,
    prop: &crate::ir::PropertyDef,
) {
    let prop_ref = SchemaEntityRef::Property {
        owner: owner_id.as_ref().to_string(),
        id: prop.id.as_str().to_string(),
    };

    // Bindings → ValueSet / CodeSystem / NotationPattern /
    // ValueRangeSet / GlossaryTerm.
    for binding in &prop.bindings {
        match binding {
            PropertyBinding::ValueSet { id, .. } => add_edge(
                edges,
                SchemaEntityRef::ValueSet { id: id.as_str().to_string() },
                prop_ref.clone(),
                DependencyKind::PropertyBindingRef,
                format!("property `{}` value-set binding", prop.name),
            ),
            PropertyBinding::CodeSystem { id, .. } => add_edge(
                edges,
                SchemaEntityRef::CodeSystem { id: id.as_str().to_string() },
                prop_ref.clone(),
                DependencyKind::PropertyBindingRef,
                format!("property `{}` code-system binding", prop.name),
            ),
            PropertyBinding::NotationPattern { id, .. } => add_edge(
                edges,
                SchemaEntityRef::NotationPattern { id: id.as_str().to_string() },
                prop_ref.clone(),
                DependencyKind::PropertyBindingRef,
                format!("property `{}` notation-pattern binding", prop.name),
            ),
            PropertyBinding::ValueRange { id, .. } => add_edge(
                edges,
                SchemaEntityRef::ValueRangeSet { id: id.as_str().to_string() },
                prop_ref.clone(),
                DependencyKind::PropertyBindingRef,
                format!("property `{}` value-range binding", prop.name),
            ),
            PropertyBinding::Glossary { id, .. } => add_edge(
                edges,
                SchemaEntityRef::GlossaryTerm { id: id.as_str().to_string() },
                prop_ref.clone(),
                DependencyKind::PropertyBindingRef,
                format!("property `{}` glossary binding", prop.name),
            ),
        }
    }

    // derived_from → Function.
    if let Some(fn_id) = &prop.derived_from {
        add_edge(
            edges,
            SchemaEntityRef::Function { id: fn_id.as_str().to_string() },
            prop_ref.clone(),
            DependencyKind::FunctionDerivation,
            format!("property `{}` derived_from", prop.name),
        );
    }

    // unit_id → CodedValue (housed inside a CodeSystem; the
    // hosting system is opaque at this layer, so we record the
    // bare CodedValue id and leave the FE to resolve via the
    // ontology's `coded_value_loc` index).
    if let Some(unit_id) = &prop.unit_id {
        add_edge(
            edges,
            SchemaEntityRef::CodedValue {
                code_system: String::new(),
                id: unit_id.as_str().to_string(),
            },
            prop_ref.clone(),
            DependencyKind::UnitReference,
            format!("property `{}` unit", prop.name),
        );
    }
}

fn walk_constraint(
    edges: &mut BTreeMap<SchemaEntityRef, Vec<DependencyEdge>>,
    rule_ref: &SchemaEntityRef,
    rule_id: &impl std::fmt::Display,
    constraint: &ShaclConstraint,
) {
    // `target` references on each constraint can pin a property
    // explicitly — record those as RuleConstraint edges so a
    // PropertyShape's dependents include cross-shape constraints
    // too.
    if let Some(ConstraintTarget::Property { node_type_id, property_id, .. }) =
        constraint_target(constraint)
    {
        add_edge(
            edges,
            SchemaEntityRef::Property {
                owner: node_type_id.as_str().to_string(),
                id: property_id.as_str().to_string(),
            },
            rule_ref.clone(),
            DependencyKind::RuleConstraint,
            format!("rule `{rule_id}` constraint"),
        );
    }

    // Vocabulary references — every cross-collection id the
    // constraint exposes via `referenced_ids()`. Adding a new
    // variant that points at a value set / notation pattern flows
    // through here automatically without touching this site.
    use crate::rule::ConstraintRef;
    for cref in constraint.referenced_ids() {
        match cref {
            ConstraintRef::ValueSet(id) => add_edge(
                edges,
                SchemaEntityRef::ValueSet { id: id.as_str().to_string() },
                rule_ref.clone(),
                DependencyKind::RuleVocabulary,
                format!("rule `{rule_id}` {}", constraint.label_kind()),
            ),
            ConstraintRef::NotationPattern(id) => add_edge(
                edges,
                SchemaEntityRef::NotationPattern { id: id.as_str().to_string() },
                rule_ref.clone(),
                DependencyKind::RuleVocabulary,
                format!("rule `{rule_id}` {}", constraint.label_kind()),
            ),
            // Sibling-property refs (LessThan / Equals) point inside
            // the rule's target NodeType — the dependency edge from
            // rule → target node already covers that, so no extra
            // edge is added here.
            ConstraintRef::PropertyId(_) => {}
        }
    }
}

fn constraint_target(c: &ShaclConstraint) -> Option<&ConstraintTarget> {
    match c {
        ShaclConstraint::MinCount { target, .. }
        | ShaclConstraint::MaxCount { target, .. }
        | ShaclConstraint::Datatype { target, .. }
        | ShaclConstraint::MatchesPattern { target, .. }
        | ShaclConstraint::InValueSet { target, .. }
        | ShaclConstraint::HasValue { target, .. }
        | ShaclConstraint::MinInclusive { target, .. }
        | ShaclConstraint::MaxInclusive { target, .. }
        | ShaclConstraint::MinLength { target, .. }
        | ShaclConstraint::MaxLength { target, .. }
        | ShaclConstraint::UniqueLang { target, .. }
        | ShaclConstraint::LessThan { target, .. }
        | ShaclConstraint::Equals { target, .. } => Some(target),
        ShaclConstraint::Closed { .. }
        | ShaclConstraint::Disjoint { .. }
        | ShaclConstraint::UniqueKey { .. } => None,
    }
}

fn link_mapping_label(kind: &LinkMappingKind) -> String {
    match kind {
        LinkMappingKind::ForeignKey { .. } => "foreign-key link".to_string(),
        LinkMappingKind::Bridge { .. } => "bridge-relation link".to_string(),
        LinkMappingKind::Computed { .. } => "computed link".to_string(),
        LinkMappingKind::Federated { .. } => "federated link".to_string(),
    }
}

fn data_quality_targets(dq: &crate::data_quality::DataQualityDef) -> Vec<SchemaEntityRef> {
    use crate::data_quality::DataQualityTarget;
    match &dq.target {
        DataQualityTarget::NodeType { node_type_id } => vec![SchemaEntityRef::NodeType {
            id: node_type_id.as_str().to_string(),
        }],
        DataQualityTarget::Property { node_type_id, property_id } => vec![SchemaEntityRef::Property {
            owner: node_type_id.as_str().to_string(),
            id: property_id.as_str().to_string(),
        }],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_user_ontology;

    #[test]
    fn build_does_not_panic_on_sample_ontology() {
        let ontology = sample_user_ontology();
        let graph = SchemaDependencyGraph::build(&ontology);
        assert!(graph.edge_count() > 0, "sample ontology must produce some edges");
    }

    #[test]
    fn property_of_edge_links_node_to_property() {
        let ontology = sample_user_ontology();
        let graph = SchemaDependencyGraph::build(&ontology);
        let user_node = ontology.node_types()[0].clone();
        let user_ref = SchemaEntityRef::NodeType {
            id: user_node.id.as_str().to_string(),
        };
        let edges = graph.dependents_of(&user_ref);
        assert!(
            edges.iter().any(|e| e.kind == DependencyKind::PropertyOf),
            "node `{}` should have property-of dependents: {edges:#?}",
            user_node.label
        );
    }

    #[test]
    fn edge_endpoints_register_source_and_target_dependencies() {
        let ontology = sample_user_ontology();
        let graph = SchemaDependencyGraph::build(&ontology);
        let mut saw_source = false;
        let mut saw_target = false;
        for edge in ontology.edge_types() {
            let src = SchemaEntityRef::NodeType {
                id: edge.source_node_id.as_str().to_string(),
            };
            let tgt = SchemaEntityRef::NodeType {
                id: edge.target_node_id.as_str().to_string(),
            };
            saw_source |= graph
                .dependents_of(&src)
                .iter()
                .any(|e| e.kind == DependencyKind::EdgeSource);
            saw_target |= graph
                .dependents_of(&tgt)
                .iter()
                .any(|e| e.kind == DependencyKind::EdgeTarget);
        }
        assert!(saw_source, "at least one EdgeSource dependency expected");
        assert!(saw_target, "at least one EdgeTarget dependency expected");
    }

    #[test]
    fn build_is_deterministic_across_invocations() {
        let ontology = sample_user_ontology();
        let g1 = SchemaDependencyGraph::build(&ontology);
        let g2 = SchemaDependencyGraph::build(&ontology);
        assert_eq!(
            serde_json::to_string(&g1).unwrap(),
            serde_json::to_string(&g2).unwrap(),
            "build output must be byte-identical across invocations"
        );
    }

    #[test]
    fn dependents_of_unknown_returns_empty_slice() {
        let ontology = sample_user_ontology();
        let graph = SchemaDependencyGraph::build(&ontology);
        let unknown = SchemaEntityRef::NodeType { id: "ghost".into() };
        assert!(graph.dependents_of(&unknown).is_empty());
    }
}

//! Agent-facing projection of [`OntologyIR`] — the slice an LLM
//! actually needs for query generation, edit drafting, and natural
//! language ↔ schema mapping.
//!
//! The full [`OntologyIR`] carries operational metadata (provenance,
//! audit trails, mapping internals, governance attributions, raw
//! `LocalizedText` shapes for every author-facing field) that an LLM
//! cannot use and that costs prompt tokens, accuracy, and latency.
//! [`AgentOntologyView`] is the canonical projection: every field
//! either informs the model's next decision or is omitted.
//!
//! ## Filtering rules
//!
//! - Deprecated entities (`deprecated_at.is_some()`) are dropped.
//! - Multilingual content collapses to the active LLM locale chain
//!   via [`LocalizedText::resolve`]; missing translations fall
//!   through to the canonical default and empty results omit.
//! - Mapping layer (object/link/property mappings) is stripped —
//!   the agent operates on the logical schema, not physical
//!   sources.
//! - Provenance, data quality, code systems / value sets / notation
//!   patterns / concept maps are stripped from the top level;
//!   their *effects* (allowed values, format hints, glossary
//!   anchors) are flattened into the relevant
//!   [`AgentPropertyView`] so the model sees what matters without
//!   the registry plumbing.
//!
//! ## Wire shape
//!
//! Every struct serialises with `skip_serializing_if` on optional
//! and empty fields so the JSON payload is dense — no
//! `"description": null` placeholders, no `"aliases": []` noise.

use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::{LanguageTag, LocalizedText};

use crate::action::RuleId;
use crate::binding::{BindingStrength, PropertyBinding};
use crate::glossary::{GlossaryTermDef, GlossaryTermId};
use crate::interface::{InterfaceDef, InterfaceId};
use crate::ir::{EdgeTypeDef, NodeTypeDef, OntologyIR, PropertyDef};
use crate::notation_pattern::{NotationPatternDef, NotationPatternId};
use crate::rule::{ConstraintTarget, RuleDef, RuleKind, ShaclConstraint};
use crate::value_set::{expand_value_set, ValueSetDef, ValueSetId};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

/// Cap on how many enum values are flattened into
/// [`AgentPropertyView::allowed_values`]. A value-set with thousands
/// of codes (ISO country, ICD diagnosis) would dominate the prompt
/// otherwise; the model sees the first slice and a hint that more
/// exist via the catalogued semantics of `value set` constraints
/// in the rule list.
const MAX_INLINE_ALLOWED_VALUES: usize = 50;

/// Cap on aliases / related terms surfaced per glossary term so a
/// single richly-tagged term doesn't dominate the prompt.
const MAX_GLOSSARY_ALIASES: usize = 8;

// ---------------------------------------------------------------------------
// Top-level view
// ---------------------------------------------------------------------------

/// Agent-facing projection of an [`OntologyIR`]. See module docs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentOntologyView {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub node_types: Vec<AgentNodeView>,
    pub edge_types: Vec<AgentEdgeView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<AgentInterfaceView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub glossary: Vec<AgentGlossaryView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<AgentRuleView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentNodeView {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
    pub properties: Vec<AgentPropertyView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentEdgeView {
    pub id: String,
    pub label: String,
    /// Source node label (not id) — the agent reasons in terms of
    /// labels, which are the surface in user questions.
    pub source: String,
    pub target: String,
    pub cardinality: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<AgentPropertyView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentPropertyView {
    /// Cypher-safe identifier (the property's `name`, not its
    /// internal `id`).
    pub name: String,
    /// Logical type — `String` / `Int` / `Float` / `Bool` / etc.
    pub property_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Synonym surface for natural language matching.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// `Measure` / `Dimension` / `Attribute` / `Identifier` —
    /// drives NL2SQL aggregation choices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation_role: Option<String>,
    /// First slice of the bound value set (when a `Required`
    /// `ValueSet` binding exists) so the model knows what literal
    /// values are valid in `WHERE p = '...'` clauses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,
    /// Notation pattern template (when a `NotationPattern`
    /// binding exists) — hint for "looks like XYZ" generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_pattern: Option<String>,
    /// Canonical glossary term name (when a `Glossary` binding
    /// exists) — strongest signal for NL → property mapping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glossary_term: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentInterfaceView {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_properties: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_edges: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentGlossaryView {
    pub term: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentRuleView {
    pub id: String,
    /// `node:<label>` / `edge:<label>` / `property:<node>.<prop>` /
    /// `cross_entity` / `state_machine` — the addressable surface
    /// of the rule.
    pub target: String,
    pub severity: String,
    /// One human-readable summary per [`ShaclConstraint`] in the
    /// rule, in declaration order.
    pub constraints: Vec<String>,
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Project this ontology into an [`AgentOntologyView`] suitable
    /// for LLM prompt injection. `locale_chain` resolves multilingual
    /// content to the workspace's `llm_locale_fallback` chain.
    pub fn to_agent_view(&self, locale_chain: &[LanguageTag]) -> AgentOntologyView {
        AgentOntologyView {
            id: self.id.clone(),
            name: self.name.clone(),
            description: present(self.description.resolve(locale_chain)),
            node_types: self
                .node_types()
                .iter()
                .filter(|n| n.deprecated_at.is_none())
                .map(|n| node_to_view(n, self, locale_chain))
                .collect(),
            edge_types: self
                .edge_types()
                .iter()
                .filter(|e| e.deprecated_at.is_none())
                .map(|e| edge_to_view(e, self, locale_chain))
                .collect(),
            interfaces: self
                .interfaces
                .iter()
                .map(|i| interface_to_view(i, self))
                .collect(),
            glossary: self
                .glossary
                .iter()
                .map(|g| glossary_to_view(g, locale_chain))
                .collect(),
            rules: self
                .rules
                .iter()
                .map(|r| rule_to_view(r, self, locale_chain))
                .collect(),
        }
    }
}

fn node_to_view(
    node: &NodeTypeDef,
    ontology: &OntologyIR,
    locale_chain: &[LanguageTag],
) -> AgentNodeView {
    AgentNodeView {
        id: node.id.as_str().to_string(),
        label: node.label.as_str().to_string(),
        description: present(node.description.resolve(locale_chain)),
        implements: node
            .implements
            .iter()
            .filter_map(|if_id| {
                ontology
                    .interfaces
                    .iter()
                    .find(|i| i.id == *if_id)
                    .map(|i| i.label.as_str().to_string())
            })
            .collect(),
        properties: node
            .properties
            .iter()
            .filter(|p| p.deprecated_at.is_none())
            .map(|p| property_to_view(p, ontology, locale_chain))
            .collect(),
    }
}

fn edge_to_view(
    edge: &EdgeTypeDef,
    ontology: &OntologyIR,
    locale_chain: &[LanguageTag],
) -> AgentEdgeView {
    AgentEdgeView {
        id: edge.id.as_str().to_string(),
        label: edge.label.as_str().to_string(),
        source: ontology
            .node_by_id(edge.source_node_id.as_str())
            .map(|n| n.label.as_str().to_string())
            .unwrap_or_else(|| edge.source_node_id.as_str().to_string()),
        target: ontology
            .node_by_id(edge.target_node_id.as_str())
            .map(|n| n.label.as_str().to_string())
            .unwrap_or_else(|| edge.target_node_id.as_str().to_string()),
        cardinality: format!("{:?}", edge.cardinality),
        description: present(edge.description.resolve(locale_chain)),
        properties: edge
            .properties
            .iter()
            .filter(|p| p.deprecated_at.is_none())
            .map(|p| property_to_view(p, ontology, locale_chain))
            .collect(),
    }
}

fn property_to_view(
    prop: &PropertyDef,
    ontology: &OntologyIR,
    locale_chain: &[LanguageTag],
) -> AgentPropertyView {
    AgentPropertyView {
        name: prop.name.as_str().to_string(),
        property_type: format!("{:?}", prop.property_type),
        nullable: prop.nullable,
        description: present(prop.description.resolve(locale_chain)),
        aliases: prop
            .aliases
            .iter()
            .map(|alias| alias.resolve(locale_chain).to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        aggregation_role: prop.aggregation_role.as_ref().map(|r| format!("{r:?}")),
        allowed_values: collect_allowed_values(prop, ontology),
        format_pattern: collect_format_pattern(prop, ontology),
        glossary_term: collect_glossary_term(prop, ontology, locale_chain),
    }
}

fn interface_to_view(iface: &InterfaceDef, _ontology: &OntologyIR) -> AgentInterfaceView {
    AgentInterfaceView {
        id: iface.id.as_str().to_string(),
        label: iface.label.as_str().to_string(),
        required_properties: iface
            .required_properties
            .iter()
            .map(|p| p.name.as_str().to_string())
            .collect(),
        required_edges: iface
            .required_edges
            .iter()
            .map(|req| req.label.as_str().to_string())
            .collect(),
    }
}

fn glossary_to_view(
    term: &GlossaryTermDef,
    locale_chain: &[LanguageTag],
) -> AgentGlossaryView {
    AgentGlossaryView {
        term: term.term.resolve(locale_chain).to_string(),
        display_name: present(term.display_name.resolve(locale_chain)),
        description: present(term.description.resolve(locale_chain)),
        aliases: term
            .aliases
            .iter()
            .map(|alias| alias.resolve(locale_chain).to_string())
            .filter(|s| !s.is_empty())
            .take(MAX_GLOSSARY_ALIASES)
            .collect(),
    }
}

fn rule_to_view(
    rule: &RuleDef,
    ontology: &OntologyIR,
    locale_chain: &[LanguageTag],
) -> AgentRuleView {
    AgentRuleView {
        id: rule.id.as_str().to_string(),
        target: rule_target(rule, ontology),
        severity: format!("{:?}", rule.severity),
        constraints: rule
            .constraints
            .iter()
            .map(|c| constraint_summary(c, ontology, locale_chain))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Property-binding flattening
// ---------------------------------------------------------------------------

fn collect_allowed_values(prop: &PropertyDef, ontology: &OntologyIR) -> Option<Vec<String>> {
    prop.bindings.iter().find_map(|b| match b {
        PropertyBinding::ValueSet {
            id,
            strength: BindingStrength::Required,
            ..
        } => ontology
            .value_set_by_id(id)
            .map(|vs| value_set_codes(vs, ontology)),
        _ => None,
    })
}

fn value_set_codes(vs: &ValueSetDef, ontology: &OntologyIR) -> Vec<String> {
    let expansion = expand_value_set(vs, ontology.code_systems());
    expansion
        .codes
        .iter()
        .map(|cv| cv.code.clone())
        .take(MAX_INLINE_ALLOWED_VALUES)
        .collect()
}

fn collect_format_pattern(prop: &PropertyDef, ontology: &OntologyIR) -> Option<String> {
    prop.bindings.iter().find_map(|b| match b {
        PropertyBinding::NotationPattern { id, .. } => ontology
            .notation_pattern_by_id(id)
            .map(notation_pattern_summary),
        _ => None,
    })
}

fn notation_pattern_summary(np: &NotationPatternDef) -> String {
    if np.template.is_empty() {
        np.name.clone()
    } else {
        np.template.clone()
    }
}

fn collect_glossary_term(
    prop: &PropertyDef,
    ontology: &OntologyIR,
    locale_chain: &[LanguageTag],
) -> Option<String> {
    prop.bindings.iter().find_map(|b| match b {
        PropertyBinding::Glossary { id, .. } => ontology
            .glossary_term_by_id(id)
            .map(|term| term.term.resolve(locale_chain).to_string()),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Rule rendering
// ---------------------------------------------------------------------------

fn rule_target(rule: &RuleDef, ontology: &OntologyIR) -> String {
    match &rule.kind {
        RuleKind::NodeShape { target_node_type_id } => ontology
            .node_by_id(target_node_type_id.as_str())
            .map(|n| format!("node:{}", n.label))
            .unwrap_or_else(|| format!("node:{target_node_type_id}")),
        RuleKind::PropertyShape {
            target_node_type_id,
            target_property_id,
        } => {
            let node = ontology.node_by_id(target_node_type_id.as_str());
            let node_label = node
                .map(|n| n.label.as_str())
                .unwrap_or(target_node_type_id.as_str());
            let prop_name = node
                .and_then(|n| n.properties.iter().find(|p| p.id == *target_property_id))
                .map(|p| p.name.as_str())
                .unwrap_or(target_property_id.as_str());
            format!("property:{node_label}.{prop_name}")
        }
        RuleKind::EdgeShape { target_edge_type_id } => ontology
            .edge_by_id(target_edge_type_id.as_str())
            .map(|e| format!("edge:{}", e.label))
            .unwrap_or_else(|| format!("edge:{target_edge_type_id}")),
        RuleKind::CrossEntityShape { .. } => "cross_entity".to_string(),
        RuleKind::StateMachine { .. } => "state_machine".to_string(),
    }
}

fn constraint_summary(
    c: &ShaclConstraint,
    ontology: &OntologyIR,
    locale_chain: &[LanguageTag],
) -> String {
    match c {
        ShaclConstraint::MinCount { min, .. } => format!("minCount = {min}"),
        ShaclConstraint::MaxCount { max, .. } => format!("maxCount = {max}"),
        ShaclConstraint::Datatype { expected, .. } => format!("datatype = {expected:?}"),
        ShaclConstraint::MatchesPattern { notation_pattern_id, .. } => ontology
            .notation_pattern_by_id(notation_pattern_id)
            .map(|np| format!("matches `{}`", notation_pattern_summary(np)))
            .unwrap_or_else(|| format!("matches `{notation_pattern_id}`")),
        ShaclConstraint::InValueSet { value_set_id, .. } => {
            let vs_label = ontology
                .value_set_by_id(value_set_id)
                .map(|vs| value_set_label(vs, locale_chain))
                .unwrap_or_else(|| value_set_id.as_str().to_string());
            format!("in `{vs_label}`")
        }
        ShaclConstraint::HasValue { value, .. } => format!("hasValue `{value}`"),
        ShaclConstraint::MinInclusive { min, .. } => format!(">= {min}"),
        ShaclConstraint::MaxInclusive { max, .. } => format!("<= {max}"),
        ShaclConstraint::MinLength { min, .. } => format!("minLength = {min}"),
        ShaclConstraint::MaxLength { max, .. } => format!("maxLength = {max}"),
        ShaclConstraint::UniqueLang { .. } => "uniqueLang".to_string(),
        ShaclConstraint::Closed { .. } => "closed shape".to_string(),
        ShaclConstraint::Disjoint { .. } => "disjoint".to_string(),
        ShaclConstraint::UniqueKey { property_keys, .. } => {
            let keys: Vec<&str> = property_keys.iter().map(|k| k.as_str()).collect();
            format!("uniqueKey ({})", keys.join(", "))
        }
        ShaclConstraint::LessThan { other_property, .. } => {
            format!("< {}", other_property.as_str())
        }
        ShaclConstraint::Equals { other_property, .. } => {
            format!("= {}", other_property.as_str())
        }
    }
}

fn value_set_label(vs: &ValueSetDef, locale_chain: &[LanguageTag]) -> String {
    let display = vs.display_name.resolve(locale_chain);
    if display.is_empty() {
        vs.name.clone()
    } else {
        display.to_string()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn present(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// `unused` import guards — kept for forward compatibility.
#[allow(dead_code)]
fn _suppress_unused_imports(
    _: HashSet<&LocalizedText>,
    _: &RuleId,
    _: &InterfaceId,
    _: &ValueSetId,
    _: &NotationPatternId,
    _: &GlossaryTermId,
    _: &ConstraintTarget,
) {
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_user_ontology;

    fn en_chain() -> Vec<LanguageTag> {
        vec![LanguageTag::en()]
    }

    #[test]
    fn agent_view_drops_deprecated_nodes() {
        let mut onto = sample_user_ontology();
        onto.node_types[0].deprecated_at = Some(chrono::Utc::now());
        let view = onto.to_agent_view(&en_chain());
        assert!(
            view.node_types.iter().all(|n| n.label != "User"),
            "deprecated User node must not surface in agent view"
        );
    }

    #[test]
    fn agent_view_resolves_descriptions_via_locale_chain() {
        let onto = sample_user_ontology();
        let view = onto.to_agent_view(&en_chain());
        // every node description, when present, is a plain string —
        // never the LocalizedText shape.
        for n in &view.node_types {
            if let Some(desc) = &n.description {
                assert!(!desc.is_empty());
            }
        }
    }

    #[test]
    fn agent_view_substantially_smaller_than_full_ir() {
        let onto = sample_user_ontology();
        let full = serde_json::to_string(&onto).unwrap();
        let view = serde_json::to_string(&onto.to_agent_view(&en_chain())).unwrap();
        assert!(
            view.len() < full.len(),
            "agent view ({} bytes) must be smaller than full IR ({} bytes)",
            view.len(),
            full.len()
        );
    }

    #[test]
    fn agent_view_omits_lookup_and_provenance_fields() {
        let onto = sample_user_ontology();
        let view_json = serde_json::to_string(&onto.to_agent_view(&en_chain())).unwrap();
        // Sanity: none of the operational fields leak through.
        assert!(!view_json.contains("\"lookup\""));
        assert!(!view_json.contains("\"provenances\""));
        assert!(!view_json.contains("\"object_mappings\""));
        assert!(!view_json.contains("\"link_mappings\""));
        assert!(!view_json.contains("\"property_mappings\""));
        assert!(!view_json.contains("\"data_qualities\""));
        assert!(!view_json.contains("\"concept_maps\""));
        assert!(!view_json.contains("\"actions\""));
        assert!(!view_json.contains("\"functions\""));
        assert!(!view_json.contains("\"metrics\""));
        assert!(!view_json.contains("\"enrichments\""));
    }

    #[test]
    fn agent_view_node_carries_label_not_id_for_edge_endpoints() {
        let onto = sample_user_ontology();
        let view = onto.to_agent_view(&en_chain());
        for edge in &view.edge_types {
            // source/target should be labels (alphabetic node names),
            // not internal `node-*` ids.
            assert!(
                !edge.source.starts_with("node-"),
                "edge source must be a label, got `{}`",
                edge.source
            );
            assert!(
                !edge.target.starts_with("node-"),
                "edge target must be a label, got `{}`",
                edge.target
            );
        }
    }
}

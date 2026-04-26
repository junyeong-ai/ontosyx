//! `RuleDef` — SHACL Core-aligned ontology constraint.
//!
//! ADR 0006 settled the rule model on SHACL so the platform could
//! inherit a W3C-backed constraint vocabulary and its ecosystem
//! (shape libraries, validators, visualisers). This module is the
//! Rust-native encoding of a subset large enough to cover every rule
//! the v3 plan needs without shipping a full SHACL engine.
//!
//! Phase 5-A scope:
//!
//! - `RuleKind` — the shape the rule is expressed as (`NodeShape`,
//!   `PropertyShape`, `EdgeShape`, `CrossEntityShape`, `StateMachine`).
//! - `ShaclConstraint` — the SHACL Core constraint components we
//!   support (cardinality, datatype, pattern, enumeration, closed,
//!   disjoint, unique-key).
//! - `Severity` / `EnforcementKind` — orthogonal to the shape:
//!   severity controls whether a violation blocks, warns, or
//!   informs; enforcement names *when* the rule runs (write-time,
//!   read-time, batch).
//! - `RuleActivationKind` — when the rule is actually live
//!   (`Always`, `OnAction`, `OnSchedule`).
//!
//! A full SHACL-SPARQL (`sh:sparql`) variant is modelled via
//! `CrossEntityShape { predicate: String }` — the string is the
//! source-dialect SQL that the planner translates to a scan, not a
//! SPARQL expression.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyType;

use crate::action::RuleId;
use crate::ir::{EdgeTypeId, NodeTypeId, PropertyId};
use crate::notation_pattern::NotationPatternId;
use crate::value_set::ValueSetId;

/// Ontology constraint, named and serializable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct RuleDef {
    pub id: RuleId,
    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    pub kind: RuleKind,

    #[serde(default)]
    pub severity: Severity,

    #[serde(default)]
    pub enforcement: EnforcementKind,

    #[serde(default)]
    pub activation: RuleActivationKind,

    /// One or more constraint components, AND'd together. Empty is
    /// syntactically valid — treated as "always passes" so the
    /// editor can save a work-in-progress rule without faking a
    /// constraint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<ShaclConstraint>,
}

/// Shape variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleKind {
    /// `sh:NodeShape` on a node type.
    NodeShape { target_node_type_id: NodeTypeId },
    /// `sh:PropertyShape` on a specific property.
    PropertyShape {
        target_node_type_id: NodeTypeId,
        target_property_id: PropertyId,
    },
    /// Ontosyx extension — constraints on an edge type (its
    /// cardinality, direction, existence). Compiles to a
    /// `NodeShape` over a synthesised edge class when exported to
    /// Turtle.
    EdgeShape { target_edge_type_id: EdgeTypeId },
    /// `sh:sparql`-style target. Expressed as a platform-native
    /// predicate because we translate to Cypher or SQL rather than
    /// SPARQL.
    CrossEntityShape {
        /// Source-dialect predicate evaluated against the scan.
        predicate: String,
    },
    /// Ontosyx extension — a state machine on a property value.
    /// Compiles to disjoint `sh:in` constraints keyed on the
    /// transition map.
    StateMachine {
        target_node_type_id: NodeTypeId,
        state_property_id: PropertyId,
        /// Allowed transitions as `(from_state, to_state)` pairs.
        /// `from_state == None` is the initial creation transition.
        transitions: Vec<StateTransition>,
    },
}

/// Named state transition inside a `StateMachine` rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct StateTransition {
    /// `None` → initial creation (no prior state).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
}

/// SHACL Core constraint component. The subset covers ~95% of
/// real-world rule usage; advanced components (`sh:and`, `sh:or`,
/// `sh:xone`, recursion rules) land in Phase 11 with the reasoning
/// engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShaclConstraint {
    /// `sh:minCount`.
    MinCount { target: ConstraintTarget, min: u32 },
    /// `sh:maxCount`.
    MaxCount { target: ConstraintTarget, max: u32 },
    /// `sh:datatype` — the value must be compatible with the given
    /// `PropertyType`. "Compatible" uses
    /// `PropertyType::check_compatibility_with`.
    Datatype { target: ConstraintTarget, expected: PropertyType },
    /// `sh:pattern` — structured format match via a referenced
    /// [`crate::notation_pattern::NotationPatternDef`]. Replaces the
    /// former free-form `regex: String` so that edits to a named
    /// pattern propagate to every rule that references it without
    /// copy-paste drift.
    MatchesPattern {
        target: ConstraintTarget,
        notation_pattern_id: NotationPatternId,
    },
    /// `sh:in` — enumerated allowed values via a referenced
    /// [`crate::value_set::ValueSetDef`]. Replaces the former
    /// free-form `allowed: Vec<String>` so a single source of truth
    /// (the value set) feeds admin UI, LLM prompts, and the runtime
    /// validator.
    InValueSet {
        target: ConstraintTarget,
        value_set_id: ValueSetId,
    },
    /// `sh:hasValue` — the property must contain this value.
    HasValue { target: ConstraintTarget, value: String },
    /// `sh:minInclusive`.
    MinInclusive { target: ConstraintTarget, min: f64 },
    /// `sh:maxInclusive`.
    MaxInclusive { target: ConstraintTarget, max: f64 },
    /// `sh:minLength`.
    MinLength { target: ConstraintTarget, min: u32 },
    /// `sh:maxLength`.
    MaxLength { target: ConstraintTarget, max: u32 },
    /// `sh:uniqueLang` — at most one literal per language tag.
    UniqueLang { target: ConstraintTarget },
    /// `sh:closed` — no properties outside the declared set are
    /// permitted on the node.
    Closed {
        target: ConstraintTarget,
        /// Properties the shape explicitly allows; anything outside
        /// this set plus the shape's property shapes is a violation.
        allowed_properties: Vec<PropertyKey>,
    },
    /// `sh:disjoint` — target A's value set does not overlap target B's.
    Disjoint {
        a: ConstraintTarget,
        b: ConstraintTarget,
    },
    /// Ontosyx extension: composite unique key (mirrors SQL
    /// `UNIQUE(col_a, col_b)`). SHACL proper expresses this through
    /// `sh:sparql`; we keep it first-class so the planner can
    /// emit a source-native unique index when the mapping allows.
    UniqueKey {
        target_node_type_id: NodeTypeId,
        property_keys: Vec<PropertyKey>,
    },
}

/// The node / property / edge a constraint component targets. Most
/// components implicitly inherit the target of their enclosing
/// `RuleDef.kind`; constraints that span multiple targets (e.g.
/// `Disjoint`) name them explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstraintTarget {
    /// Inherited from the enclosing `RuleDef.kind`. Most constraints
    /// use this.
    Inherit,
    Property {
        node_type_id: NodeTypeId,
        property_id: PropertyId,
    },
    NodeType {
        node_type_id: NodeTypeId,
    },
    EdgeLabel {
        label: GraphLabel,
    },
}

/// Violation severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Rule failure blocks the operation (write rejected, query
    /// fails, batch validation report marks the row invalid).
    #[default]
    Violation,
    /// Rule failure is surfaced as a warning. UI highlights the row;
    /// the operation itself still proceeds.
    Warning,
    /// Rule failure is purely informational — helpful for telemetry
    /// (SLA miss counters, etc.) without user-visible friction.
    Info,
}

/// When the rule runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementKind {
    /// Pre-execute check on a mutation. Failure aborts the write.
    #[default]
    Write,
    /// Post-scan check on a read. Failure surfaces as a
    /// `ResultIssue`; rows stay in the result, the caller decides
    /// how to react.
    Read,
    /// Periodic reconciliation by the data-quality scheduler. Results
    /// land in `DataQualityDef` reports, not in user request
    /// responses.
    Batch,
}

/// When the rule is live at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleActivationKind {
    /// Always active.
    #[default]
    Always,
    /// Active only while a specific action is executing. Used for
    /// `ActionDef.preconditions` / `postconditions`.
    OnAction { action_id: crate::action::ActionId },
    /// Active on a cron schedule (batch enforcement). `Batch`
    /// enforcement + `OnSchedule` activation is the natural pairing.
    OnSchedule { cron_expression: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_severity_is_violation() {
        assert_eq!(Severity::default(), Severity::Violation);
    }

    #[test]
    fn default_enforcement_is_write() {
        assert_eq!(EnforcementKind::default(), EnforcementKind::Write);
    }

    #[test]
    fn node_shape_rule_round_trips() {
        let r = RuleDef {
            id: RuleId::new("r-email-format"),
            name: "email_is_valid".into(),
            description: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![
                ShaclConstraint::MinCount {
                    target: ConstraintTarget::Inherit,
                    min: 1,
                },
                ShaclConstraint::MatchesPattern {
                    target: ConstraintTarget::Inherit,
                    notation_pattern_id: NotationPatternId::new("np-email"),
                },
            ],
        };
        let j = serde_json::to_value(&r).unwrap();
        let back: RuleDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn state_machine_rule_preserves_transition_set() {
        let r = RuleDef {
            id: RuleId::new("r-order-state"),
            name: "order_state_machine".into(),
            description: LocalizedText::default(),
            kind: RuleKind::StateMachine {
                target_node_type_id: NodeTypeId::new("nt-order"),
                state_property_id: PropertyId::new("prop-status"),
                transitions: vec![
                    StateTransition {
                        from: None,
                        to: "draft".into(),
                    },
                    StateTransition {
                        from: Some("draft".into()),
                        to: "submitted".into(),
                    },
                    StateTransition {
                        from: Some("submitted".into()),
                        to: "paid".into(),
                    },
                ],
            },
            severity: Severity::default(),
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![],
        };
        let j = serde_json::to_value(&r).unwrap();
        let back: RuleDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn unique_key_is_representable_as_a_first_class_variant() {
        let c = ShaclConstraint::UniqueKey {
            target_node_type_id: NodeTypeId::new("nt-invoice"),
            property_keys: vec![
                PropertyKey::new("invoice_no").unwrap(),
                PropertyKey::new("vendor_id").unwrap(),
            ],
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"kind\":\"unique_key\""));
    }
}

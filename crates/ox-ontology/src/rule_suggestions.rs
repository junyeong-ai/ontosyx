//! SHACL rule suggestions derived from registry changes.
//!
//! When an operator adds or updates a registry entry (a
//! `ValueSetDef`, a `NotationPatternDef`, a glossary binding, a
//! `ConceptMapDef`), the platform can usually propose a
//! corresponding SHACL `RuleDef` before the operator has to hunt
//! through the rule catalogue:
//!
//! - A new `ValueSetDef` bound to a property → propose an
//!   `InValueSet` constraint on the binding.
//! - A new `NotationPatternDef` bound to a property → propose a
//!   `MatchesPattern` constraint.
//! - A `GlossaryTermDef` wired to one or more properties →
//!   propose a `MinCount{min:1}` rule for each if the property is
//!   non-nullable and unruled.
//!
//! This module is the **pure engine**. The caller decides whether
//! to persist proposals, show them as a banner, or silently drop
//! them. Persistence lives in a separate store trait so this
//! module stays decoupled from the DB.
//!
//! Output shape: each `RuleProposal` carries a fully-formed
//! `RuleDef` plus provenance — which change triggered the
//! suggestion and a short human-readable rationale. Applying a
//! proposal is equivalent to calling `add_rule(proposal.rule)` on
//! the ontology.

use crate::action::RuleId;
use crate::ir::{OntologyIR, PropertyDef};
use crate::notation_pattern::NotationPatternId;
use crate::rule::{
    ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity,
    ShaclConstraint,
};
use crate::value_set::ValueSetId;
use ox_core::i18n::LocalizedText;

/// Trigger that prompts a suggestion pass. Kept as a tagged enum so
/// the same engine can serve every registry-change flow with a
/// single call site.
#[derive(Debug, Clone)]
pub enum RegistryChange {
    /// A `ValueSetDef` was added or bound to a property.
    ValueSetAttached {
        value_set_id: ValueSetId,
    },
    /// A `NotationPatternDef` was added or bound to a property.
    NotationPatternAttached {
        notation_pattern_id: NotationPatternId,
    },
}

/// A single suggested rule. The admin UI typically renders these
/// in a "review and apply" list; `rule.id` is pre-populated with a
/// deterministic prefix (`"auto-<kind>-<id>"`) so repeated runs
/// converge on the same set instead of flooding the UI with
/// duplicates.
#[derive(Debug, Clone)]
pub struct RuleProposal {
    pub rule: RuleDef,
    pub trigger: RuleProposalTrigger,
    pub rationale: String,
}

/// Concise provenance for one proposal — surfaces in the UI's
/// "why did we suggest this?" tooltip.
#[derive(Debug, Clone)]
pub enum RuleProposalTrigger {
    ValueSetBinding {
        value_set_id: ValueSetId,
        property_id: String,
    },
    NotationPatternBinding {
        notation_pattern_id: NotationPatternId,
        property_id: String,
    },
}

/// Run one suggestion pass against the current ontology.
///
/// The function is pure. It does **not** consult any external
/// registry; every lookup resolves against the `ontology` argument.
/// The caller is responsible for staging the change (e.g. adding a
/// new `ValueSetDef`) before invoking this function.
pub fn suggest_rules_for_change(
    ontology: &OntologyIR,
    change: RegistryChange,
) -> Vec<RuleProposal> {
    match change {
        RegistryChange::ValueSetAttached { value_set_id } => {
            suggest_for_value_set(ontology, &value_set_id)
        }
        RegistryChange::NotationPatternAttached {
            notation_pattern_id,
        } => suggest_for_notation_pattern(ontology, &notation_pattern_id),
    }
}

fn suggest_for_value_set(ontology: &OntologyIR, vs_id: &ValueSetId) -> Vec<RuleProposal> {
    let Some(vs) = ontology.value_set_by_id(vs_id) else {
        return Vec::new();
    };

    let already_ruled = existing_value_set_rules(ontology, vs_id);
    let mut out = Vec::new();

    for (node_id, prop) in properties_bound_to_value_set(ontology, vs_id) {
        if already_ruled.contains(prop.id.as_str()) {
            continue;
        }
        let rule_id = RuleId::new(format!("auto-vs-{}-{}", vs_id.as_str(), prop.id.as_str()));
        let rule = RuleDef {
            id: rule_id,
            name: format!("enforce_{vs_name}_on_{prop_name}",
                vs_name = vs.name,
                prop_name = prop.name.as_str()),
            description: LocalizedText::new(format!(
                "Auto-generated from value-set binding on `{prop}`.",
                prop = prop.name.as_str()
            )),
            kind: RuleKind::PropertyShape {
                target_node_type_id: node_id.clone(),
                target_property_id: prop.id.clone(),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: vs_id.clone(),
            }],
        };
        out.push(RuleProposal {
            rule,
            trigger: RuleProposalTrigger::ValueSetBinding {
                value_set_id: vs_id.clone(),
                property_id: prop.id.to_string(),
            },
            rationale: format!(
                "Property `{prop}` is bound to value set `{vs}`; enforce the \
                 bound codes at write time.",
                prop = prop.name.as_str(),
                vs = vs.name,
            ),
        });
    }

    out
}

fn suggest_for_notation_pattern(
    ontology: &OntologyIR,
    np_id: &NotationPatternId,
) -> Vec<RuleProposal> {
    let Some(np) = ontology.notation_pattern_by_id(np_id) else {
        return Vec::new();
    };

    let already_ruled = existing_pattern_rules(ontology, np_id);
    let mut out = Vec::new();

    for (node_id, prop) in properties_bound_to_pattern(ontology, np_id) {
        if already_ruled.contains(prop.id.as_str()) {
            continue;
        }
        let rule_id = RuleId::new(format!(
            "auto-np-{}-{}",
            np_id.as_str(),
            prop.id.as_str()
        ));
        let rule = RuleDef {
            id: rule_id,
            name: format!(
                "enforce_{np_name}_on_{prop_name}",
                np_name = np.name,
                prop_name = prop.name.as_str()
            ),
            description: LocalizedText::new(format!(
                "Auto-generated from notation-pattern binding on `{prop}`.",
                prop = prop.name.as_str()
            )),
            kind: RuleKind::PropertyShape {
                target_node_type_id: node_id.clone(),
                target_property_id: prop.id.clone(),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::MatchesPattern {
                target: ConstraintTarget::Inherit,
                notation_pattern_id: np_id.clone(),
            }],
        };
        out.push(RuleProposal {
            rule,
            trigger: RuleProposalTrigger::NotationPatternBinding {
                notation_pattern_id: np_id.clone(),
                property_id: prop.id.to_string(),
            },
            rationale: format!(
                "Property `{prop}` is bound to notation pattern `{np}`; enforce \
                 the format at write time.",
                prop = prop.name.as_str(),
                np = np.name,
            ),
        });
    }

    out
}

fn properties_bound_to_value_set<'a>(
    ontology: &'a OntologyIR,
    vs_id: &ValueSetId,
) -> Vec<(crate::ir::NodeTypeId, &'a PropertyDef)> {
    let mut out = Vec::new();
    for node in ontology.node_types() {
        for prop in &node.properties {
            if prop.value_set_id.as_ref() == Some(vs_id) {
                out.push((node.id.clone(), prop));
            }
        }
    }
    out
}

fn properties_bound_to_pattern<'a>(
    ontology: &'a OntologyIR,
    np_id: &NotationPatternId,
) -> Vec<(crate::ir::NodeTypeId, &'a PropertyDef)> {
    let mut out = Vec::new();
    for node in ontology.node_types() {
        for prop in &node.properties {
            if prop.notation_pattern_id.as_ref() == Some(np_id) {
                out.push((node.id.clone(), prop));
            }
        }
    }
    out
}

fn existing_value_set_rules(
    ontology: &OntologyIR,
    vs_id: &ValueSetId,
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for rule in ontology.rules() {
        let RuleKind::PropertyShape {
            target_property_id, ..
        } = &rule.kind
        else {
            continue;
        };
        for c in &rule.constraints {
            if let ShaclConstraint::InValueSet { value_set_id, .. } = c
                && value_set_id == vs_id
            {
                out.insert(target_property_id.to_string());
            }
        }
    }
    out
}

fn existing_pattern_rules(
    ontology: &OntologyIR,
    np_id: &NotationPatternId,
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for rule in ontology.rules() {
        let RuleKind::PropertyShape {
            target_property_id, ..
        } = &rule.kind
        else {
            continue;
        };
        for c in &rule.constraints {
            if let ShaclConstraint::MatchesPattern {
                notation_pattern_id,
                ..
            } = c
                && notation_pattern_id == np_id
            {
                out.insert(target_property_id.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId};
    use crate::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId};
    use crate::notation_pattern::{
        NotationComponent, NotationComponentKind, NotationPatternDef,
    };
    use crate::value_set::{IncludeMode, ValueSetDef, ValueSetIncludeRule, ValueSetSelector};
    use ox_core::graph_label::GraphLabel;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    fn property_bound_to_vs(vs_id: ValueSetId) -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("p-status"),
            name: PropertyKey::new("status").unwrap(),
            property_type: PropertyType::String,
            value_set_id: Some(vs_id),
            ..Default::default()
        }
    }

    fn property_bound_to_pattern(np_id: NotationPatternId) -> PropertyDef {
        PropertyDef {
            id: PropertyId::new("p-code"),
            name: PropertyKey::new("code").unwrap(),
            property_type: PropertyType::String,
            notation_pattern_id: Some(np_id),
            ..Default::default()
        }
    }

    fn ontology_with_vs(vs_id: ValueSetId, prop: PropertyDef) -> OntologyIR {
        let cs = CodeSystemDef {
            id: CodeSystemId::new("cs-status"),
            name: "cs".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes: vec![CodedValue {
                id: CodedValueId::new("cv-a"),
                code: "A".into(),
                display: LocalizedText::default(),
                definition: LocalizedText::default(),
                aliases: vec![],
                broader_id: None,
                examples: vec![],
                scope_note: LocalizedText::default(),
                valid_from: None,
                valid_to: None,
                deprecated_at: None,
                replaced_by_id: None,
            }],
            deprecated_at: None,
            replaced_by_id: None,
        };
        let vs = ValueSetDef {
            id: vs_id.clone(),
            name: "vs-status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("cs-status"),
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            }],
        };
        let mut ir = OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt-user"),
                label: GraphLabel::new("User").unwrap(),
                properties: vec![prop],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        ir.add_code_system(cs).expect("add code system");
        ir.add_value_set(vs).expect("add value set");
        ir
    }

    #[test]
    fn value_set_attachment_proposes_in_value_set_rule() {
        let vs_id = ValueSetId::new("vs-status");
        let ir = ontology_with_vs(vs_id.clone(), property_bound_to_vs(vs_id.clone()));
        let proposals = suggest_rules_for_change(
            &ir,
            RegistryChange::ValueSetAttached {
                value_set_id: vs_id.clone(),
            },
        );
        assert_eq!(proposals.len(), 1);
        let p = &proposals[0];
        assert!(matches!(
            p.rule.constraints[0],
            ShaclConstraint::InValueSet { .. }
        ));
        assert!(matches!(p.trigger, RuleProposalTrigger::ValueSetBinding { .. }));
        assert!(p.rationale.contains("value set"));
    }

    #[test]
    fn value_set_already_ruled_is_skipped() {
        let vs_id = ValueSetId::new("vs-status");
        let mut ir = ontology_with_vs(vs_id.clone(), property_bound_to_vs(vs_id.clone()));
        // Pre-add the expected rule — proposer must not duplicate.
        ir.add_rule(RuleDef {
            id: RuleId::new("r-existing"),
            name: "existing".into(),
            description: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("p-status"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            constraints: vec![ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: vs_id.clone(),
            }],
        })
        .expect("add rule");
        let proposals = suggest_rules_for_change(
            &ir,
            RegistryChange::ValueSetAttached {
                value_set_id: vs_id,
            },
        );
        assert!(proposals.is_empty());
    }

    #[test]
    fn unbound_value_set_yields_no_proposals() {
        let vs_id = ValueSetId::new("vs-orphan");
        // Build an ontology where no property points at the target value
        // set. The engine must return an empty list, not synthesize
        // one from thin air.
        let cs = CodeSystemDef {
            id: CodeSystemId::new("cs"),
            name: "cs".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes: Vec::new(),
            deprecated_at: None,
            replaced_by_id: None,
        };
        let vs = ValueSetDef {
            id: vs_id.clone(),
            name: "vs-orphan".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("cs"),
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            }],
        };
        let mut ir = OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt"),
                label: GraphLabel::new("N").unwrap(),
                properties: vec![PropertyDef {
                    id: PropertyId::new("p"),
                    name: PropertyKey::new("name").unwrap(),
                    property_type: PropertyType::String,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        ir.add_code_system(cs).unwrap();
        ir.add_value_set(vs).unwrap();
        let proposals = suggest_rules_for_change(
            &ir,
            RegistryChange::ValueSetAttached {
                value_set_id: vs_id,
            },
        );
        assert!(proposals.is_empty());
    }

    #[test]
    fn notation_pattern_attachment_proposes_matches_pattern_rule() {
        let np_id = NotationPatternId::new("np-code");
        let np = NotationPatternDef {
            id: np_id.clone(),
            name: "np-code".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            template: "{value:5}".into(),
            separator: String::new(),
            components: vec![NotationComponent {
                name: "value".into(),
                display: LocalizedText::default(),
                kind: NotationComponentKind::Alphanumeric {
                    width: 5,
                    uppercase: false,
                },
            }],
            examples: Vec::new(),
        };
        let mut ir = OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt"),
                label: GraphLabel::new("N").unwrap(),
                properties: vec![property_bound_to_pattern(np_id.clone())],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        ir.add_notation_pattern(np).unwrap();
        let proposals = suggest_rules_for_change(
            &ir,
            RegistryChange::NotationPatternAttached {
                notation_pattern_id: np_id,
            },
        );
        assert_eq!(proposals.len(), 1);
        assert!(matches!(
            proposals[0].rule.constraints[0],
            ShaclConstraint::MatchesPattern { .. }
        ));
    }
}

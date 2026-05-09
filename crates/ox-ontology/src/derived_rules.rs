//! Synthesise SHACL `RuleDef`s as the platform's safety-net layer
//! over schema-level invariants — `Required`-strength
//! [`PropertyBinding`]s and the `nullable=false` flag on
//! [`PropertyDef`].
//!
//! The two derivation axes:
//!
//! - **Binding**: a [`PropertyBinding`] with `BindingStrength::Required`
//!   *promises* every write satisfies the binding's domain — but
//!   until the ontology author also authors a matching `RuleDef`,
//!   no validator enforces that promise. This module closes that
//!   gap by turning the binding itself into the rule.
//! - **Nullable**: `PropertyDef.nullable=false` is the schema-level
//!   NOT NULL declaration. Without a derivation step it would only
//!   surface at the planner's source-mapping layer (where the
//!   typer reads it as a column-side invariant) and never reach
//!   the SHACL validator's write path. This module derives an
//!   implicit `MinCount=1` rule so a `CREATE (u:User)` missing a
//!   non-null property is rejected pre-execute, the same way an
//!   authored `MinCount=1` rule would.
//!
//! Mapping from [`PropertyBinding`] variant to [`ShaclConstraint`]:
//!
//! - `ValueSet { strength: Required, id }` → `InValueSet { value_set_id }`
//! - `NotationPattern { strength: Required, id }` → `MatchesPattern`
//! - `CodeSystem { strength: Required, .. }` → no derived rule (no
//!   SHACL constraint variant covers "any code from this system" —
//!   wrap the system in a value set to enforce)
//! - `ValueRange` / `Glossary` → no derived rule, by shape: their
//!   variants don't carry a `strength` field at all (ranges
//!   classify, glossary is a semantic anchor)
//!
//! The synthesised rules carry `Severity::Violation`,
//! `EnforcementKind::Write`, `RuleActivationKind::Always`. Their
//! `valid_from`/`valid_to` mirror the binding's temporal window so
//! `OntologyIR::as_of` filtering applies identically to authored and
//! derived rules.
//!
//! Synthesised `RuleId`s use the deterministic prefix `derived:binding:`
//! followed by node + property + target ids, so the same binding
//! always yields the same id across calls (idempotent — safe to feed
//! into hash-keyed validators).

use std::collections::HashSet;

use ox_core::i18n::LocalizedText;

use crate::action::RuleId;
use crate::binding::{BindingStrength, PropertyBinding};
use crate::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId};
use crate::rule::{
    ConstraintSignature, ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind,
    RuleOrigin, Severity, ShaclConstraint,
};

/// Prefix marking a synthesised rule. Consumers can filter authored
/// from derived rules without checking each id by hand.
pub const DERIVED_BINDING_RULE_PREFIX: &str = "derived:binding:";

impl OntologyIR {
    /// Synthesise SHACL rules from every `Required`-strength
    /// `PropertyBinding` whose target maps to an enforceable
    /// `ShaclConstraint`. See module docs for the mapping table.
    ///
    /// **Single source of truth for dedup**: the function suppresses
    /// derivations whose `(node, property, signature)` is already
    /// covered by an authored rule in `self.rules()`. Authored rules
    /// carry explicit user intent (custom severity, message, name);
    /// derivations are the platform's safety net. Letting both fire
    /// would surface duplicate violations to the LLM and the admin
    /// UI. Suppressing here means every downstream consumer
    /// (validator, exporter, suggester, LLM context builder) can
    /// iterate the merged set without re-implementing the dedup.
    ///
    /// Always returns a fresh `Vec` — the rules are not memoised
    /// because callers (the SHACL validator, exporters, advisory
    /// tooling) typically iterate them once per request and the
    /// computation is O(node_types × properties × bindings).
    /// Synthesise SHACL rules along the `Required`-strength
    /// [`PropertyBinding`] axis only. Use [`Self::derive_implicit_rules`]
    /// to reach the full safety-net set (binding + nullable).
    pub fn derive_binding_rules(&self) -> Vec<RuleDef> {
        let authored = self.authored_constraint_signatures();
        let mut out = Vec::new();
        for node in &self.node_types {
            for prop in &node.properties {
                for binding in &prop.bindings {
                    let Some(rule) = derive_binding_rule(node, prop, binding) else {
                        continue;
                    };
                    if rule_redundant_with_authored(&rule, &authored) {
                        continue;
                    }
                    out.push(rule);
                }
            }
        }
        out
    }

    /// Synthesise SHACL rules along the `nullable=false` axis only.
    /// Use [`Self::derive_implicit_rules`] to reach the full
    /// safety-net set (binding + nullable).
    pub fn derive_nullable_rules(&self) -> Vec<RuleDef> {
        let authored = self.authored_constraint_signatures();
        let mut out = Vec::new();
        for node in &self.node_types {
            for prop in &node.properties {
                if let Some(rule) = derive_nullable_rule(node, prop)
                    && !rule_redundant_with_authored(&rule, &authored)
                {
                    out.push(rule);
                }
            }
        }
        out
    }

    /// Full safety-net set — [`Self::derive_binding_rules`] +
    /// [`Self::derive_nullable_rules`]. Consumers that enforce write
    /// invariants (the SHACL validator) call this; consumers that
    /// reason only about a single derivation axis (rule-suggestion
    /// engines that filter by binding kind) call the per-axis
    /// methods directly.
    pub fn derive_implicit_rules(&self) -> Vec<RuleDef> {
        let mut out = self.derive_binding_rules();
        out.extend(self.derive_nullable_rules());
        out
    }

    /// Authored constraint signatures keyed by **effective property
    /// target**. Used by `derive_implicit_rules` to suppress
    /// redundant derivations and by the rule-suggestion engine to
    /// skip proposals already covered.
    ///
    /// "Effective" means [`effective_property_target`] semantics: an
    /// `InValueSet { target: Property{n, p}, ... }` constraint
    /// inside a `PropertyShape` rule whose nominal target is
    /// `(n, q)` keys at `(n, p)` — the constraint applies to `p`,
    /// not the rule's nominal `q`. Without this resolution, a
    /// derived rule on `p` would over-fire alongside the authored
    /// rule's effective enforcement on `p`.
    ///
    /// Constraints whose [`ShaclConstraint::signature`] returns
    /// `None` (dedup-independent kinds) contribute nothing — their
    /// presence in an authored rule must not silently suppress an
    /// orthogonal derived rule on the same property.
    pub fn authored_constraint_signatures(
        &self,
    ) -> HashSet<(NodeTypeId, PropertyId, ConstraintSignature)> {
        let mut out = HashSet::new();
        for rule in self.rules() {
            if !matches!(rule.origin, RuleOrigin::Authored) {
                continue;
            }
            for c in &rule.constraints {
                if let (Some((node_id, prop_id)), Some(sig)) =
                    (effective_property_target(rule, c), c.signature())
                {
                    out.insert((node_id, prop_id, sig));
                }
            }
        }
        out
    }

    /// `(node, property, signature)` triples enforced by derived
    /// rules, keyed by effective property target. The rule-
    /// suggestion engine uses this to skip proposing authored rules
    /// that would be redundant with the platform's safety-net
    /// derivation.
    pub fn derived_constraint_signatures(
        &self,
    ) -> HashSet<(NodeTypeId, PropertyId, ConstraintSignature)> {
        let mut out = HashSet::new();
        for rule in self.derive_implicit_rules() {
            for c in &rule.constraints {
                if let (Some((node_id, prop_id)), Some(sig)) =
                    (effective_property_target(&rule, c), c.signature())
                {
                    out.insert((node_id, prop_id, sig));
                }
            }
        }
        out
    }
}

/// Resolve the property a constraint actually enforces against,
/// honouring `ConstraintTarget::Property{...}` overrides.
///
/// Resolution table:
/// - `target = Inherit` + `RuleKind::PropertyShape{n, p}` → `Some((n, p))`
/// - `target = Inherit` + any other rule kind → `None`
///   (Inherit on a NodeShape/EdgeShape doesn't refer to a single
///   property; the constraint either is node-level or invalid)
/// - `target = Property{n, p}` → `Some((n, p))` (regardless of rule
///   kind — a NodeShape can legitimately carry a property-level
///   constraint via explicit target)
/// - `target = NodeType{...}` / `EdgeLabel{...}` → `None`
///   (constraint enforces against the type itself, not a property)
/// - constraint has no single target (`Disjoint`, `UniqueKey`) → `None`
fn effective_property_target(
    rule: &RuleDef,
    constraint: &ShaclConstraint,
) -> Option<(NodeTypeId, PropertyId)> {
    let target = constraint.target()?;
    match target {
        ConstraintTarget::Inherit => match &rule.kind {
            RuleKind::PropertyShape {
                target_node_type_id,
                target_property_id,
            } => Some((target_node_type_id.clone(), target_property_id.clone())),
            _ => None,
        },
        ConstraintTarget::Property {
            node_type_id,
            property_id,
        } => Some((node_type_id.clone(), property_id.clone())),
        ConstraintTarget::NodeType { .. } | ConstraintTarget::EdgeLabel { .. } => None,
    }
}

/// Bilingual name for a derived rule. Both strings are produced
/// from the IR labels deterministically — no LLM round-trip, no
/// human authoring. Resolution semantics match
/// [`LocalizedText::bilingual`]: the admin chain (`["ko", "en"]`)
/// reads Korean; the LLM chain (`["en", "ko"]`) reads English.
fn derived_rule_name(node: &NodeTypeDef, prop: &PropertyDef) -> LocalizedText {
    let label = node.label.as_str();
    let prop_name = prop.name.as_str();
    LocalizedText::bilingual(
        format!("{label}.{prop_name} — 필수 바인딩 자동 적용"),
        format!("{label}.{prop_name} required-binding enforcement"),
    )
}

fn rule_redundant_with_authored(
    rule: &RuleDef,
    authored: &HashSet<(NodeTypeId, PropertyId, ConstraintSignature)>,
) -> bool {
    rule.constraints.iter().any(|c| {
        let Some((node_id, prop_id)) = effective_property_target(rule, c) else {
            return false;
        };
        c.signature()
            .is_some_and(|sig| authored.contains(&(node_id, prop_id, sig)))
    })
}

/// Derive a SHACL rule from the schema-level `nullable=false`
/// declaration on a property. The derivation is conditional:
/// `nullable=true` produces nothing, and the caller suppresses
/// emission when an authored MinCount rule already enforces the
/// same property (the dedup pipeline keys on
/// `ConstraintSignature::MinCount`).
///
/// The derivation deliberately does NOT cover MaxCount — graph
/// instance multiplicity is not implied by `nullable`, and an
/// authored MaxCount expresses a separate axis that the safety
/// net should not infer.
fn derive_nullable_rule(node: &NodeTypeDef, prop: &PropertyDef) -> Option<RuleDef> {
    if prop.nullable {
        return None;
    }
    let id = RuleId::new(format!(
        "{DERIVED_BINDING_RULE_PREFIX}{node}:{property}:nullable",
        node = node.id.as_str(),
        property = prop.id.as_str(),
    ));
    Some(RuleDef {
        id,
        name: derived_rule_name(node, prop),
        description: LocalizedText::default(),
        rationale: LocalizedText::bilingual(
            format!(
                "속성 `{label}.{name}` 의 `nullable=false` 선언에서 자동 파생된 강제 규칙입니다.",
                label = node.label,
                name = prop.name
            ),
            format!(
                "Auto-derived enforcement of `nullable=false` on \
                 `{label}.{name}`.",
                label = node.label,
                name = prop.name
            ),
        ),
        kind: RuleKind::PropertyShape {
            target_node_type_id: node.id.clone(),
            target_property_id: prop.id.clone(),
        },
        severity: Severity::Violation,
        enforcement: EnforcementKind::Write,
        activation: RuleActivationKind::Always,
        origin: RuleOrigin::DerivedFromBinding {
            node_type_id: node.id.clone(),
            property_id: prop.id.clone(),
        },
        constraints: vec![ShaclConstraint::MinCount {
            target: ConstraintTarget::Inherit,
            min: 1,
        }],
        valid_from: None,
        valid_to: None,
        sh_message: None,
    })
}

fn derive_binding_rule(
    node: &NodeTypeDef,
    prop: &PropertyDef,
    binding: &PropertyBinding,
) -> Option<RuleDef> {
    // Only the two enforceable, signature-aligned target kinds
    // produce a derived rule. ValueRange/Glossary are intentionally
    // outside the safety-net surface (see binding.rs docs); the
    // CodeSystem case lacks a single SHACL constraint variant —
    // wrap it in a value set if you need enforcement.
    let (constraint, target_suffix, valid_from, valid_to) = match binding {
        PropertyBinding::ValueSet {
            id,
            strength: BindingStrength::Required,
            valid_from,
            valid_to,
            ..
        } => (
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: id.clone(),
            },
            format!("value_set:{}", id.as_str()),
            *valid_from,
            *valid_to,
        ),
        PropertyBinding::NotationPattern {
            id,
            strength: BindingStrength::Required,
            valid_from,
            valid_to,
        } => (
            ShaclConstraint::MatchesPattern {
                target: ConstraintTarget::Inherit,
                notation_pattern_id: id.clone(),
            },
            format!("notation_pattern:{}", id.as_str()),
            *valid_from,
            *valid_to,
        ),
        _ => return None,
    };

    let id = RuleId::new(format!(
        "{DERIVED_BINDING_RULE_PREFIX}{node}:{property}:{target}",
        node = node.id.as_str(),
        property = prop.id.as_str(),
        target = target_suffix,
    ));

    Some(RuleDef {
        id,
        name: derived_rule_name(node, prop),
        description: LocalizedText::default(),
        rationale: LocalizedText::bilingual(
            format!(
                "속성 `{label}.{name}` 의 Required 바인딩에서 자동 파생된 강제 규칙입니다.",
                label = node.label,
                name = prop.name
            ),
            format!(
                "Auto-derived enforcement of the Required binding on \
                 `{label}.{name}`.",
                label = node.label,
                name = prop.name
            ),
        ),
        kind: RuleKind::PropertyShape {
            target_node_type_id: node.id.clone(),
            target_property_id: prop.id.clone(),
        },
        severity: Severity::Violation,
        enforcement: EnforcementKind::Write,
        activation: RuleActivationKind::Always,
        origin: RuleOrigin::DerivedFromBinding {
            node_type_id: node.id.clone(),
            property_id: prop.id.clone(),
        },
        constraints: vec![constraint],
        valid_from,
        valid_to,
        sh_message: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::binding::{BindingStrength, PropertyBinding};
    use crate::code_system::{
        CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId,
    };
    use crate::ir::{NodeTypeDef, OntologyIR, PropertyDef};
    use crate::rule::{RuleKind, ShaclConstraint};
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    fn ontology_with_property(name: &str) -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "DerivedRulesTest".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: "nt-x".into(),
                label: GraphLabel::new("X").unwrap(),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p-x".into(),
                    name: PropertyKey::new(name).unwrap(),
                    property_type: PropertyType::String,
                    nullable: false,
                    ..Default::default()
                }],
                constraints: Vec::new(),
                ..Default::default()
            }],
            Vec::new(),
            Vec::new(),
        )
        .expect("seed ontology")
    }

    fn add_singleton_code_system(ontology: &mut OntologyIR, id: &str) -> CodeSystemId {
        let cs_id = CodeSystemId::new(id);
        ontology
            .add_code_system(CodeSystemDef {
                id: cs_id.clone(),
                name: id.to_string(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                version: "1".into(),
                kind: CodeSystemKind::Internal,
                uri: None,
                hierarchical: false,
                codes: vec![CodedValue {
                    id: CodedValueId::new(format!("cv-{id}")),
                    code: "ACTIVE".into(),
                    display: LocalizedText::default(),
                    definition: LocalizedText::default(),
                    aliases: Vec::new(),
                    broader_id: None,
                    examples: Vec::new(),
                    scope_note: LocalizedText::default(),
                    valid_from: None,
                    valid_to: None,
                    deprecated_at: None,
                    replaced_by_id: None,
                }],
                deprecated_at: None,
                replaced_by_id: None,
            })
            .unwrap();
        cs_id
    }

    fn push_binding(ontology: &mut OntologyIR, binding: PropertyBinding) {
        ontology
            .node_types_mut()
            .iter_mut()
            .next()
            .unwrap()
            .properties[0]
            .bindings
            .push(binding);
        ontology.rebuild_indices().unwrap();
    }

    #[test]
    fn code_system_required_binding_yields_no_derived_rule_today() {
        // CodeSystem-targeted Required bindings are deliberately
        // outside the derived-rule surface — the SHACL constraint
        // vocabulary supports `InValueSet` (a value set wraps codes
        // selected from one or more code systems) but no direct
        // "any code from this system" variant. Wrapping the system
        // in a value set is the supported escape hatch.
        //
        // This test pins that limitation so a future engineer adding
        // CodeSystem support knows exactly which assertion to flip.
        let mut ontology = ontology_with_property("status");
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        push_binding(
            &mut ontology,
            PropertyBinding::CodeSystem {
                id: cs_id,
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );

        let derived = ontology.derive_binding_rules();
        assert!(
            derived.is_empty(),
            "CodeSystem Required binding must not synthesise today: {derived:?}"
        );
    }

    #[test]
    fn concept_binding_yields_no_derived_rule() {
        // Concept bindings are semantic anchors — they declare
        // "this property realises this concept" without imposing a
        // value-domain constraint. Synthesising a SHACL rule from
        // them would be a heuristic lie.
        use crate::concept::{ConceptDef, ConceptGovernance, ConceptId};
        use crate::glossary::{GlossaryTermDef, GlossaryTermId};

        let mut ontology = ontology_with_property("status");
        let term_id = GlossaryTermId::new("gt-customer-status");
        let concept_id = ConceptId::new("c-customer-status");
        ontology
            .add_glossary_term(GlossaryTermDef {
                id: term_id.clone(),
                term: LocalizedText::new("CustomerStatus"),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                examples: Vec::new(),
                category: None,
                aliases: Vec::new(),
                related_terms: Vec::new(),
                governance: crate::glossary::TermGovernance::default(),
                valid_from: None,
                valid_to: None,
                lifecycle: crate::glossary::TermLifecycle::default(),
                concept_id: Some(concept_id.clone()),
                term_pos: Default::default(),
            })
            .unwrap();
        ontology
            .add_concept(ConceptDef {
                id: concept_id.clone(),
                canonical_term_id: term_id,
                alias_term_ids: Vec::new(),
                broader: None,
                description: LocalizedText::default(),
                examples: Vec::new(),
                category: None,
                realisation: None,
                lifecycle: crate::glossary::TermLifecycle::default(),
                replaced_by: None,
                valid_from: None,
                valid_to: None,
                governance: ConceptGovernance::default(),
            })
            .unwrap();
        push_binding(&mut ontology, PropertyBinding::concept(concept_id));

        let derived = ontology.derive_binding_rules();
        assert!(derived.is_empty(), "{derived:?}");
    }

    #[test]
    fn derived_rule_carries_provenance_back_to_source_binding() {
        use crate::value_set::{
            IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
        };

        let mut ontology = ontology_with_property("status");
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = ValueSetId::new("vs-status");
        ontology
            .add_value_set(ValueSetDef {
                id: vs_id.clone(),
                name: "status".into(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                version: "1".into(),
                composition: vec![ValueSetIncludeRule {
                    mode: IncludeMode::Include,
                    system_id: cs_id,
                    selector: ValueSetSelector::All,
                }],
            })
            .unwrap();
        push_binding(
            &mut ontology,
            PropertyBinding::ValueSet {
                id: vs_id.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );

        let derived = ontology.derive_binding_rules();
        assert_eq!(derived.len(), 1);
        let rule = &derived[0];

        assert!(matches!(
            &rule.origin,
            RuleOrigin::DerivedFromBinding { node_type_id, property_id }
            if node_type_id.as_str() == "nt-x" && property_id.as_str() == "p-x"
        ));
        assert!(matches!(
            &rule.kind,
            RuleKind::PropertyShape { target_node_type_id, target_property_id }
            if target_node_type_id.as_str() == "nt-x" && target_property_id.as_str() == "p-x"
        ));
        assert!(matches!(
            rule.constraints.as_slice(),
            [ShaclConstraint::InValueSet { value_set_id, .. }]
            if value_set_id.as_str() == vs_id.as_str()
        ));
        assert!(rule.id.as_str().starts_with(DERIVED_BINDING_RULE_PREFIX));
    }

    // ----- Wave 8.12 — single-source-of-truth dedup -----

    /// Helper: seed an authored PropertyShape rule with the given
    /// constraint on the fixture's `(nt-x, p-x)` coordinates.
    fn add_authored_rule(ontology: &mut OntologyIR, id: &str, constraint: ShaclConstraint) {
        use crate::action::RuleId;
        use crate::rule::{EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity};
        ontology
            .add_rule(RuleDef {
                id: RuleId::new(id),
                name: id.into(),
                description: LocalizedText::default(),
                rationale: LocalizedText::default(),
                kind: RuleKind::PropertyShape {
                    target_node_type_id: "nt-x".into(),
                    target_property_id: "p-x".into(),
                },
                severity: Severity::Violation,
                enforcement: EnforcementKind::Write,
                activation: RuleActivationKind::Always,
                origin: RuleOrigin::Authored,
                constraints: vec![constraint],
                valid_from: None,
                valid_to: None,
                sh_message: None,
            })
            .expect("authored rule add");
    }

    fn seed_value_set_with_id(
        ontology: &mut OntologyIR,
        cs_id: &CodeSystemId,
        vs_label: &str,
    ) -> crate::value_set::ValueSetId {
        use crate::value_set::{
            IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
        };
        let vs_id = ValueSetId::new(vs_label);
        ontology
            .add_value_set(ValueSetDef {
                id: vs_id.clone(),
                name: vs_label.into(),
                display_name: LocalizedText::default(),
                description: LocalizedText::default(),
                version: "1".into(),
                composition: vec![ValueSetIncludeRule {
                    mode: IncludeMode::Include,
                    system_id: cs_id.clone(),
                    selector: ValueSetSelector::All,
                }],
            })
            .unwrap();
        vs_id
    }

    #[test]
    fn derived_rule_skipped_when_authored_in_value_set_already_covers() {
        // Setup: Required binding on `vs-status` PLUS an authored
        // rule whose constraint is also `InValueSet(vs-status)`.
        // The derived rule and the authored rule have the same
        // signature on the same property — derivation must suppress
        // itself, leaving only the authored rule active.
        let mut ontology = ontology_with_property("status");
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = seed_value_set_with_id(&mut ontology, &cs_id, "vs-status");

        push_binding(
            &mut ontology,
            PropertyBinding::ValueSet {
                id: vs_id.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        add_authored_rule(
            &mut ontology,
            "r-authored-status",
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: vs_id.clone(),
            },
        );

        let derived = ontology.derive_binding_rules();
        assert!(
            derived.is_empty(),
            "derivation must suppress when authored signature matches: {derived:?}",
        );
    }

    #[test]
    fn derived_rule_kept_when_authored_targets_different_value_set_id() {
        // Authored rule covers a DIFFERENT value set than the binding
        // points at. Derivation has no overlap and must still emit.
        let mut ontology = ontology_with_property("status");
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_bound = seed_value_set_with_id(&mut ontology, &cs_id, "vs-bound");
        let vs_other = seed_value_set_with_id(&mut ontology, &cs_id, "vs-other");

        push_binding(
            &mut ontology,
            PropertyBinding::ValueSet {
                id: vs_bound.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        add_authored_rule(
            &mut ontology,
            "r-other",
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: vs_other.clone(),
            },
        );

        let derived = ontology.derive_binding_rules();
        assert_eq!(derived.len(), 1);
        assert!(matches!(
            derived[0].constraints.as_slice(),
            [ShaclConstraint::InValueSet { value_set_id, .. }]
            if value_set_id.as_str() == vs_bound.as_str()
        ));
    }

    #[test]
    fn derived_rule_kept_when_authored_uses_dedup_independent_constraint_kind() {
        // Authored rule uses `MinCount` (signature `None`,
        // dedup-independent) on the same property; derivation's
        // `InValueSet(Some)` signature has no overlap.
        //
        // The contract: dedup-independent constraints (`signature
        // == None`) MUST NOT collapse against any other constraint
        // — including other dedup-independent ones. This test
        // pins that two `None`-signed constraints don't accidentally
        // suppress derivation through a shared "catch-all" bucket.
        let mut ontology = ontology_with_property("status");
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = seed_value_set_with_id(&mut ontology, &cs_id, "vs-status");

        push_binding(
            &mut ontology,
            PropertyBinding::ValueSet {
                id: vs_id.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        add_authored_rule(
            &mut ontology,
            "r-min-count",
            ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 1,
            },
        );

        let derived = ontology.derive_binding_rules();
        assert_eq!(
            derived.len(),
            1,
            "MinCount and InValueSet are orthogonal intents: {derived:?}",
        );
        // MinCount carries its own dedup signature (so the implicit
        // nullable=false derivation can suppress correctly), but its
        // signature is `MinCount` — orthogonal to the binding's
        // `InValueSet(id)` signature, so the InValueSet derivation
        // here must still emit. The signature index reflects only
        // the authored MinCount's contribution.
        let authored_sigs = ontology.authored_constraint_signatures();
        assert_eq!(authored_sigs.len(), 1);
        assert!(
            authored_sigs
                .iter()
                .any(|(_, _, sig)| matches!(sig, crate::rule::ConstraintSignature::MinCount))
        );
    }

    // ----- Wave 8.14 — effective-target dedup -----

    /// Two-property ontology fixture used by the explicit-target
    /// tests. Properties: `p-x` (the binding-bound property) and
    /// `p-y` (the authored rule's nominal target).
    fn ontology_with_two_properties() -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "TwoProps".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: "nt-x".into(),
                label: ox_core::graph_label::GraphLabel::new("X").unwrap(),
                description: LocalizedText::default(),
                properties: vec![
                    PropertyDef {
                        id: "p-x".into(),
                        name: ox_core::property_key::PropertyKey::new("p_x").unwrap(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: false,
                        ..Default::default()
                    },
                    PropertyDef {
                        id: "p-y".into(),
                        name: ox_core::property_key::PropertyKey::new("p_y").unwrap(),
                        property_type: ox_core::types::PropertyType::String,
                        nullable: false,
                        ..Default::default()
                    },
                ],
                constraints: Vec::new(),
                ..Default::default()
            }],
            Vec::new(),
            Vec::new(),
        )
        .expect("seed ontology")
    }

    /// Add a PropertyShape rule whose nominal target is the given
    /// `nominal_property_id`, carrying a single constraint with the
    /// supplied `target` override. Lets the test spell out the
    /// nominal/explicit-target divergence the dedup walker must
    /// resolve.
    fn add_property_shape_rule_with_constraint_target(
        ontology: &mut OntologyIR,
        rule_id: &str,
        nominal_property_id: &str,
        constraint: ShaclConstraint,
    ) {
        use crate::action::RuleId;
        use crate::rule::{EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity};
        ontology
            .add_rule(RuleDef {
                id: RuleId::new(rule_id),
                name: rule_id.into(),
                description: LocalizedText::default(),
                rationale: LocalizedText::default(),
                kind: RuleKind::PropertyShape {
                    target_node_type_id: "nt-x".into(),
                    target_property_id: nominal_property_id.into(),
                },
                severity: Severity::Violation,
                enforcement: EnforcementKind::Write,
                activation: RuleActivationKind::Always,
                origin: RuleOrigin::Authored,
                constraints: vec![constraint],
                valid_from: None,
                valid_to: None,
                sh_message: None,
            })
            .expect("authored rule add");
    }

    fn add_node_shape_rule_with_constraint(
        ontology: &mut OntologyIR,
        rule_id: &str,
        constraint: ShaclConstraint,
    ) {
        use crate::action::RuleId;
        use crate::rule::{EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity};
        ontology
            .add_rule(RuleDef {
                id: RuleId::new(rule_id),
                name: rule_id.into(),
                description: LocalizedText::default(),
                rationale: LocalizedText::default(),
                kind: RuleKind::NodeShape {
                    target_node_type_id: "nt-x".into(),
                },
                severity: Severity::Violation,
                enforcement: EnforcementKind::Write,
                activation: RuleActivationKind::Always,
                origin: RuleOrigin::Authored,
                constraints: vec![constraint],
                valid_from: None,
                valid_to: None,
                sh_message: None,
            })
            .expect("authored node-shape rule add");
    }

    fn push_binding_on(ontology: &mut OntologyIR, property_id: &str, binding: PropertyBinding) {
        let prop = ontology
            .node_types_mut()
            .iter_mut()
            .next()
            .unwrap()
            .properties
            .iter_mut()
            .find(|p| p.id.as_str() == property_id)
            .expect("property exists");
        prop.bindings.push(binding);
        ontology.rebuild_indices().unwrap();
    }

    #[test]
    fn derived_rule_dedups_against_authored_constraint_with_explicit_property_target_override() {
        // Authored: PropertyShape nominally on (nt-x, p-y) carrying
        // an `InValueSet` constraint whose `target` overrides to
        // (nt-x, p-x). The constraint EFFECTIVELY enforces p-x.
        // Derived: Required binding on p-x → PropertyShape on
        // (nt-x, p-x) with InValueSet/Inherit. Both effectively
        // enforce the same intent on p-x — the derivation must
        // dedup itself away.
        let mut ontology = ontology_with_two_properties();
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = seed_value_set_with_id(&mut ontology, &cs_id, "vs-status");

        push_binding_on(
            &mut ontology,
            "p-x",
            PropertyBinding::ValueSet {
                id: vs_id.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        add_property_shape_rule_with_constraint_target(
            &mut ontology,
            "r-with-explicit-target",
            "p-y", // nominal target is p-y
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Property {
                    node_type_id: "nt-x".into(),
                    property_id: "p-x".into(), // but constraint enforces p-x
                },
                value_set_id: vs_id.clone(),
            },
        );

        let derived = ontology.derive_binding_rules();
        assert!(
            derived.is_empty(),
            "derivation must defer to the authored constraint's \
             effective target on p-x: {derived:?}",
        );
    }

    #[test]
    fn derived_rule_kept_when_authored_constraint_targets_unrelated_property() {
        // Authored: PropertyShape nominally on p-y with explicit-
        // target InValueSet on p-y. Derived: binding on p-x.
        // The two enforce different effective properties — the
        // derivation must be preserved.
        let mut ontology = ontology_with_two_properties();
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = seed_value_set_with_id(&mut ontology, &cs_id, "vs-status");

        push_binding_on(
            &mut ontology,
            "p-x",
            PropertyBinding::ValueSet {
                id: vs_id.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        add_property_shape_rule_with_constraint_target(
            &mut ontology,
            "r-on-p-y",
            "p-y",
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Property {
                    node_type_id: "nt-x".into(),
                    property_id: "p-y".into(),
                },
                value_set_id: vs_id.clone(),
            },
        );

        let derived = ontology.derive_binding_rules();
        assert_eq!(derived.len(), 1);
    }

    #[test]
    fn node_shape_rule_with_explicit_property_constraint_dedups_derived_rule() {
        // SHACL allows a NodeShape to carry a property-level
        // constraint via explicit `ConstraintTarget::Property{...}`.
        // The dedup pipeline must recognise the effective target
        // even when the rule kind is NodeShape rather than
        // PropertyShape.
        let mut ontology = ontology_with_two_properties();
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = seed_value_set_with_id(&mut ontology, &cs_id, "vs-status");

        push_binding_on(
            &mut ontology,
            "p-x",
            PropertyBinding::ValueSet {
                id: vs_id.clone(),
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        add_node_shape_rule_with_constraint(
            &mut ontology,
            "r-node-shape-prop",
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Property {
                    node_type_id: "nt-x".into(),
                    property_id: "p-x".into(),
                },
                value_set_id: vs_id.clone(),
            },
        );

        let derived = ontology.derive_binding_rules();
        assert!(
            derived.is_empty(),
            "NodeShape with explicit Property-target constraint \
             must suppress the equivalent derivation: {derived:?}",
        );
    }

    // ---- Wave 8.15 — nullable=false derivation -----------------------

    fn ontology_with_nullable(name: &str, nullable: bool) -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "NullableTest".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: "nt-x".into(),
                label: GraphLabel::new("X").unwrap(),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p-x".into(),
                    name: PropertyKey::new(name).unwrap(),
                    property_type: PropertyType::String,
                    nullable,
                    ..Default::default()
                }],
                constraints: Vec::new(),
                ..Default::default()
            }],
            Vec::new(),
            Vec::new(),
        )
        .expect("seed ontology")
    }

    #[test]
    fn nullable_false_derives_min_count_one() {
        let ontology = ontology_with_nullable("status", false);
        let derived = ontology.derive_nullable_rules();
        assert_eq!(derived.len(), 1, "nullable=false must derive: {derived:?}");
        let rule = &derived[0];
        assert!(matches!(
            &rule.constraints[0],
            ShaclConstraint::MinCount { min: 1, .. }
        ));
        assert!(matches!(
            &rule.origin,
            RuleOrigin::DerivedFromBinding { property_id, .. } if property_id.as_str() == "p-x"
        ));
    }

    #[test]
    fn nullable_true_derives_nothing() {
        let ontology = ontology_with_nullable("optional_field", true);
        let derived = ontology.derive_nullable_rules();
        assert!(
            derived.is_empty(),
            "nullable=true must not derive: {derived:?}"
        );
    }

    #[test]
    fn nullable_derivation_suppressed_when_authored_min_count_present() {
        // An authored MinCount=2 already implies the platform's
        // implicit MinCount=1; the nullable safety net must not
        // pile on a duplicate diagnostic.
        let mut ontology = ontology_with_nullable("status", false);
        add_authored_rule(
            &mut ontology,
            "r-min-count-explicit",
            ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 2,
            },
        );
        let derived = ontology.derive_nullable_rules();
        assert!(
            derived.is_empty(),
            "authored MinCount must suppress nullable derivation: {derived:?}",
        );
    }

    #[test]
    fn implicit_rules_set_unions_binding_and_nullable() {
        // Property has BOTH a Required ValueSet binding (binding axis)
        // AND nullable=false (nullable axis). `derive_implicit_rules`
        // must emit one derivation per axis — the validator wants the
        // full safety net.
        let mut ontology = ontology_with_nullable("status", false);
        let cs_id = add_singleton_code_system(&mut ontology, "cs-status");
        let vs_id = seed_value_set_with_id(&mut ontology, &cs_id, "vs-status");
        push_binding(
            &mut ontology,
            PropertyBinding::ValueSet {
                id: vs_id,
                strength: BindingStrength::Required,
                concept_map_id: None,
                valid_from: None,
                valid_to: None,
            },
        );
        let implicit = ontology.derive_implicit_rules();
        assert_eq!(
            implicit.len(),
            2,
            "binding + nullable axes must each contribute: {implicit:?}",
        );
        assert!(
            implicit
                .iter()
                .any(|r| matches!(&r.constraints[0], ShaclConstraint::InValueSet { .. }))
        );
        assert!(
            implicit
                .iter()
                .any(|r| matches!(&r.constraints[0], ShaclConstraint::MinCount { min: 1, .. }))
        );
    }
}

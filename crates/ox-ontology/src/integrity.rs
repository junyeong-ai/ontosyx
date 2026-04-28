//! Registry cross-reference integrity.
//!
//! As the ontology grew past a flat node/edge/property model, a
//! thicket of optional id-pointers appeared on `PropertyDef` and
//! elsewhere:
//!
//! - `PropertyDef.glossary_term_id` → `GlossaryTermDef.id`
//! - `PropertyDef.value_set_id` → `ValueSetDef.id`
//! - `PropertyDef.notation_pattern_id` → `NotationPatternDef.id`
//! - `PropertyDef.value_range_set_id` → `ValueRangeSetDef.id`
//! - `PropertyDef.unit_id` → a `CodedValueId` inside some `CodeSystemDef`
//! - `ShaclConstraint::InValueSet` → `ValueSetDef.id`
//! - `ShaclConstraint::MatchesPattern` → `NotationPatternDef.id`
//! - `ValueSetIncludeRule.system_id` → `CodeSystemDef.id`
//! - `ConceptMapDef.source_system_id` / `target_system_id` → `CodeSystemDef.id`
//!
//! Any one of those can drift when an operator deletes a registry
//! entry that a property still references, or an import step
//! fabricates an id that was never added. Dangling references
//! produce subtle runtime symptoms (the SHACL validator silently
//! skips rules; the LLM prompt RAG layer drops property enrichment)
//! rather than loud errors, which makes them hard to notice.
//!
//! This module walks every known id-pointer and collects the
//! dangling ones into a single report. The report plugs into
//! `OntologyIR::validate()` so save-time validation refuses any IR
//! that would otherwise ship a broken reference.

use ox_core::diagnostic::{diag, DiagnosticMessage};

use crate::code_system::{CodeSystemDef, CodedValueId};
use crate::concept_map::ConceptMapDef;
use crate::glossary::GlossaryTermId;
use crate::ir::{EdgeTypeDef, NodeTypeDef, OntologyIR, PropertyDef};
use crate::notation_pattern::NotationPatternId;
use crate::rule::{RuleDef, ShaclConstraint};
#[cfg(test)]
use crate::rule::ConstraintTarget;
use crate::value_range::ValueRangeSetId;
use crate::value_set::{ValueSetDef, ValueSetId, ValueSetSelector};
use crate::code_system::CodeSystemId;

/// One dangling id pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanglingReference {
    pub source: RegistrySite,
    pub kind: DanglingKind,
    pub missing_id: String,
}

/// Where the dangling pointer lives. Rendered in error messages so
/// an operator can jump straight to the offending entity without a
/// second lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrySite {
    Property {
        owner_label: String,
        property_name: String,
        field: &'static str,
    },
    Rule {
        rule_name: String,
        constraint_kind: &'static str,
    },
    ValueSet {
        value_set_name: String,
        rule_index: usize,
    },
    ConceptMap {
        concept_map_name: String,
        field: &'static str,
    },
}

/// The kind of collection the pointer *should* have landed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DanglingKind {
    GlossaryTerm,
    ValueSet,
    NotationPattern,
    ValueRangeSet,
    CodeSystem,
    CodedValue,
}

/// Trait implemented by any container that exposes a registry
/// cross-reference surface. `OntologyIR` is the primary consumer;
/// individual diff / edit helpers can reuse the same trait when
/// they need to pre-check a partial IR before saving.
pub trait RegistryReferenceCheck {
    /// Return every dangling reference in the container.
    fn dangling_references(&self) -> Vec<DanglingReference>;
}

impl RegistryReferenceCheck for OntologyIR {
    fn dangling_references(&self) -> Vec<DanglingReference> {
        let glossary: std::collections::HashSet<&GlossaryTermId> =
            self.glossary().iter().map(|g| &g.id).collect();
        let value_sets: std::collections::HashMap<&ValueSetId, &ValueSetDef> =
            self.value_sets().iter().map(|vs| (&vs.id, vs)).collect();
        let notation_patterns: std::collections::HashSet<&NotationPatternId> =
            self.notation_patterns().iter().map(|np| &np.id).collect();
        let value_ranges: std::collections::HashSet<&ValueRangeSetId> =
            self.value_range_sets().iter().map(|r| &r.id).collect();
        let code_systems: std::collections::HashMap<&CodeSystemId, &CodeSystemDef> =
            self.code_systems().iter().map(|cs| (&cs.id, cs)).collect();

        let mut out = Vec::new();

        // ------------------------------------------------------
        // PropertyDef pointers.
        // ------------------------------------------------------
        let node_owners = self
            .node_types()
            .iter()
            .map(|n: &NodeTypeDef| (n.label.as_str().to_string(), &n.properties));
        let edge_owners = self
            .edge_types()
            .iter()
            .map(|e: &EdgeTypeDef| (e.label.as_str().to_string(), &e.properties));
        for (owner_label, properties) in node_owners.chain(edge_owners) {
            for p in properties {
                property_pointer_walk(
                    &owner_label,
                    p,
                    &glossary,
                    &value_sets,
                    &notation_patterns,
                    &value_ranges,
                    &code_systems,
                    &mut out,
                );
            }
        }

        // ------------------------------------------------------
        // RuleDef.constraints pointers.
        // ------------------------------------------------------
        for rule in self.rules() {
            rule_pointer_walk(
                rule,
                &value_sets,
                &notation_patterns,
                &mut out,
            );
        }

        // ------------------------------------------------------
        // ValueSet composition → CodeSystem + optional named codes.
        // ------------------------------------------------------
        for vs in self.value_sets() {
            value_set_composition_walk(vs, &code_systems, &mut out);
        }

        // ------------------------------------------------------
        // ConceptMap → source / target systems.
        // ------------------------------------------------------
        for cm in self.concept_maps() {
            concept_map_walk(cm, &code_systems, &mut out);
        }

        out
    }
}

fn property_pointer_walk(
    owner_label: &str,
    p: &PropertyDef,
    glossary: &std::collections::HashSet<&GlossaryTermId>,
    value_sets: &std::collections::HashMap<&ValueSetId, &ValueSetDef>,
    notation_patterns: &std::collections::HashSet<&NotationPatternId>,
    value_ranges: &std::collections::HashSet<&ValueRangeSetId>,
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    out: &mut Vec<DanglingReference>,
) {
    let site = |field: &'static str| RegistrySite::Property {
        owner_label: owner_label.to_string(),
        property_name: p.name.as_str().to_string(),
        field,
    };

    // Single walk over the property's binding list — every target
    // kind is checked through the same loop, with the field tag
    // pointing at the binding variant so admin reports stay specific.
    for binding in &p.bindings {
        match binding {
            crate::binding::PropertyBinding::Glossary { id, .. }
                if !glossary.contains(id) =>
            {
                out.push(DanglingReference {
                    source: site("bindings.glossary"),
                    kind: DanglingKind::GlossaryTerm,
                    missing_id: id.to_string(),
                });
            }
            crate::binding::PropertyBinding::ValueSet { id, .. }
                if !value_sets.contains_key(id) =>
            {
                out.push(DanglingReference {
                    source: site("bindings.value_set"),
                    kind: DanglingKind::ValueSet,
                    missing_id: id.to_string(),
                });
            }
            crate::binding::PropertyBinding::NotationPattern { id, .. }
                if !notation_patterns.contains(id) =>
            {
                out.push(DanglingReference {
                    source: site("bindings.notation_pattern"),
                    kind: DanglingKind::NotationPattern,
                    missing_id: id.to_string(),
                });
            }
            crate::binding::PropertyBinding::ValueRange { id, .. }
                if !value_ranges.contains(id) =>
            {
                out.push(DanglingReference {
                    source: site("bindings.value_range"),
                    kind: DanglingKind::ValueRangeSet,
                    missing_id: id.to_string(),
                });
            }
            crate::binding::PropertyBinding::CodeSystem { .. } => {
                // CodeSystem registry membership is checked by
                // `OntologyIR::validate()` directly against
                // `code_system_id_idx`; this walker doesn't carry
                // a CodeSystem index because the dangling-ref
                // surface here pre-dates the CodeSystem binding
                // variant.
            }
            _ => {}
        }
    }
    if let Some(uid) = &p.unit_id
        && !coded_value_exists(code_systems, uid)
    {
        out.push(DanglingReference {
            source: site("unit_id"),
            kind: DanglingKind::CodedValue,
            missing_id: uid.to_string(),
        });
    }
}

fn rule_pointer_walk(
    rule: &RuleDef,
    value_sets: &std::collections::HashMap<&ValueSetId, &ValueSetDef>,
    notation_patterns: &std::collections::HashSet<&NotationPatternId>,
    out: &mut Vec<DanglingReference>,
) {
    use crate::rule::ConstraintRef;

    // Walk every cross-collection reference each constraint exposes
    // through `referenced_ids()`. Adding a new constraint variant
    // that points at a value set / notation pattern / sibling
    // property requires one match arm in `ShaclConstraint::referenced_ids`
    // — the integrity pass picks the new ref up automatically.
    //
    // PropertyId references (e.g. `LessThan.other_property`) are
    // resolved in `OntologyIR::validate`'s rule pass against the
    // owning node's properties; this walker only handles the
    // sibling-collection refs (ValueSet, NotationPattern).
    for constraint in &rule.constraints {
        for cref in constraint.referenced_ids() {
            match cref {
                ConstraintRef::ValueSet(id) => {
                    if !value_sets.contains_key(id) {
                        out.push(DanglingReference {
                            source: RegistrySite::Rule {
                                rule_name: rule.name.default.clone(),
                                constraint_kind: constraint_kind_static(constraint),
                            },
                            kind: DanglingKind::ValueSet,
                            missing_id: id.to_string(),
                        });
                    }
                }
                ConstraintRef::NotationPattern(id) => {
                    if !notation_patterns.contains(id) {
                        out.push(DanglingReference {
                            source: RegistrySite::Rule {
                                rule_name: rule.name.default.clone(),
                                constraint_kind: constraint_kind_static(constraint),
                            },
                            kind: DanglingKind::NotationPattern,
                            missing_id: id.to_string(),
                        });
                    }
                }
                ConstraintRef::PropertyId(_) => {
                    // PropertyId references on rule constraints are
                    // resolved against the owning node's property
                    // list during `OntologyIR::validate` — handled
                    // there to keep this walker focused on
                    // cross-collection ids.
                }
            }
        }
    }
}

/// Map a constraint to a `&'static str` "kind" that can be embedded
/// in the [`RegistrySite::Rule`] variant. Mirrors `label_kind` in
/// PascalCase so the diagnostic copy reads naturally
/// ("constraint_kind: InValueSet").
fn constraint_kind_static(c: &ShaclConstraint) -> &'static str {
    match c {
        ShaclConstraint::InValueSet { .. } => "InValueSet",
        ShaclConstraint::MatchesPattern { .. } => "MatchesPattern",
        ShaclConstraint::LessThan { .. } => "LessThan",
        ShaclConstraint::Equals { .. } => "Equals",
        // Other variants don't reach this helper because they have
        // no cross-collection refs; keeping the arm explicit catches
        // any future variant that does.
        ShaclConstraint::MinCount { .. }
        | ShaclConstraint::MaxCount { .. }
        | ShaclConstraint::Datatype { .. }
        | ShaclConstraint::HasValue { .. }
        | ShaclConstraint::MinInclusive { .. }
        | ShaclConstraint::MaxInclusive { .. }
        | ShaclConstraint::MinLength { .. }
        | ShaclConstraint::MaxLength { .. }
        | ShaclConstraint::UniqueLang { .. }
        | ShaclConstraint::Closed { .. }
        | ShaclConstraint::Disjoint { .. }
        | ShaclConstraint::UniqueKey { .. } => "Other",
    }
}

fn value_set_composition_walk(
    vs: &ValueSetDef,
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    out: &mut Vec<DanglingReference>,
) {
    for (idx, rule) in vs.composition.iter().enumerate() {
        if !code_systems.contains_key(&rule.system_id) {
            out.push(DanglingReference {
                source: RegistrySite::ValueSet {
                    value_set_name: vs.name.clone(),
                    rule_index: idx,
                },
                kind: DanglingKind::CodeSystem,
                missing_id: rule.system_id.to_string(),
            });
            continue;
        }
        // For Explicit selectors we could also check that every
        // named code exists in the system, but that check is
        // expensive (O(codes × systems)) and rewards partial
        // matches with non-actionable noise on imports. The runtime
        // `expand_value_set` surfaces missing codes through its
        // own warning channel — that's the right place.
        let _ = &rule.selector;
        let _ = ValueSetSelector::All; // keep the variant used
    }
}

fn concept_map_walk(
    cm: &ConceptMapDef,
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    out: &mut Vec<DanglingReference>,
) {
    if !code_systems.contains_key(&cm.source_system_id) {
        out.push(DanglingReference {
            source: RegistrySite::ConceptMap {
                concept_map_name: cm.name.clone(),
                field: "source_system_id",
            },
            kind: DanglingKind::CodeSystem,
            missing_id: cm.source_system_id.to_string(),
        });
    }
    if !code_systems.contains_key(&cm.target_system_id) {
        out.push(DanglingReference {
            source: RegistrySite::ConceptMap {
                concept_map_name: cm.name.clone(),
                field: "target_system_id",
            },
            kind: DanglingKind::CodeSystem,
            missing_id: cm.target_system_id.to_string(),
        });
    }
}

fn coded_value_exists(
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    id: &CodedValueId,
) -> bool {
    code_systems
        .values()
        .any(|cs| cs.codes.iter().any(|cv| cv.id == *id))
}

/// Render dangling-reference reports as structured
/// [`DiagnosticMessage`]s used by `OntologyIR::validate()`. Each
/// diagnostic carries:
///
/// - `code = "ontology.validate.integrity.dangling_<kind>"` —
///   stable handle the FE catalogue keys against.
/// - `params.site_kind`, `params.site_*` fields — describe where
///   the dangling pointer was found (property / rule / value set /
///   concept map) so the FE can deep-link.
/// - `params.missing_id` — the unresolved id.
pub fn render_dangling_references(refs: &[DanglingReference]) -> Vec<DiagnosticMessage> {
    refs.iter()
        .map(|r| {
            let kind = match r.kind {
                DanglingKind::GlossaryTerm => "glossary_term",
                DanglingKind::ValueSet => "value_set",
                DanglingKind::NotationPattern => "notation_pattern",
                DanglingKind::ValueRangeSet => "value_range_set",
                DanglingKind::CodeSystem => "code_system",
                DanglingKind::CodedValue => "coded_value",
            };
            let (site_label, mut builder) = match &r.source {
                RegistrySite::Property {
                    owner_label,
                    property_name,
                    field,
                } => (
                    format!("property `{owner_label}.{property_name}.{field}`"),
                    diag(format!("ontology.validate.integrity.dangling_{kind}"))
                        .with("site_kind", "property")
                        .with("owner_label", owner_label.clone())
                        .with("property_name", property_name.clone())
                        .with("field", *field),
                ),
                RegistrySite::Rule {
                    rule_name,
                    constraint_kind,
                } => (
                    format!("rule `{rule_name}` ({constraint_kind} constraint)"),
                    diag(format!("ontology.validate.integrity.dangling_{kind}"))
                        .with("site_kind", "rule")
                        .with("rule_name", rule_name.clone())
                        .with("constraint_kind", *constraint_kind),
                ),
                RegistrySite::ValueSet {
                    value_set_name,
                    rule_index,
                } => (
                    format!("value set `{value_set_name}` (composition rule {rule_index})"),
                    diag(format!("ontology.validate.integrity.dangling_{kind}"))
                        .with("site_kind", "value_set")
                        .with("value_set_name", value_set_name.clone())
                        .with("rule_index", *rule_index as u64),
                ),
                RegistrySite::ConceptMap {
                    concept_map_name,
                    field,
                } => (
                    format!("concept map `{concept_map_name}.{field}`"),
                    diag(format!("ontology.validate.integrity.dangling_{kind}"))
                        .with("site_kind", "concept_map")
                        .with("concept_map_name", concept_map_name.clone())
                        .with("field", *field),
                ),
            };
            builder = builder
                .with("kind", kind)
                .with("missing_id", r.missing_id.clone());
            builder.message(format!(
                "{site_label} references unknown {kind} `{missing}`",
                missing = r.missing_id,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind};
    use crate::glossary::GlossaryTermId;
    use crate::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId};
    use crate::rule::{RuleOrigin, 
        EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity, ShaclConstraint,
    };
    use crate::action::RuleId;
    use crate::value_set::ValueSetId;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    fn p_with(name: &str, f: impl FnOnce(&mut PropertyDef)) -> PropertyDef {
        let mut p = PropertyDef {
            id: PropertyId::new(format!("p-{name}")),
            name: PropertyKey::new(name).unwrap(),
            property_type: PropertyType::String,
            ..Default::default()
        };
        f(&mut p);
        p
    }

    fn ontology_with_properties(props: Vec<PropertyDef>) -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt"),
                label: GraphLabel::new("N").unwrap(),
                properties: props,
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid seed ontology")
    }

    #[test]
    fn empty_ontology_has_no_dangling_refs() {
        let ir = ontology_with_properties(vec![p_with("name", |_| {})]);
        assert!(ir.dangling_references().is_empty());
    }

    #[test]
    fn property_with_missing_glossary_term_surfaces() {
        let ir = ontology_with_properties(vec![p_with("name", |p| {
            p.bindings.push(crate::binding::PropertyBinding::glossary(GlossaryTermId::new("g-missing"),));
        })]);
        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::GlossaryTerm);
        assert_eq!(refs[0].missing_id, "g-missing");
    }

    #[test]
    fn property_with_missing_value_set_surfaces() {
        let ir = ontology_with_properties(vec![p_with("status", |p| {
            p.bindings.push(crate::binding::PropertyBinding::value_set(ValueSetId::new("vs-missing"),));
        })]);
        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::ValueSet);
    }

    #[test]
    fn rule_referencing_missing_value_set_surfaces() {
        let mut ir = ontology_with_properties(vec![p_with("status", |_| {})]);
        ir.add_rule(RuleDef {
            id: RuleId::new("r"),
            name: "enum_status".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt"),
                target_property_id: PropertyId::new("p-status"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![ShaclConstraint::InValueSet {
                target: ConstraintTarget::Inherit,
                value_set_id: ValueSetId::new("vs-missing"),
            }],
            valid_from: None,
            valid_to: None,
                    sh_message: None,
        })
        .expect("rule add");
        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert!(matches!(&refs[0].source, RegistrySite::Rule { .. }));
    }

    #[test]
    fn unit_id_without_hosting_code_system_surfaces() {
        let ir = ontology_with_properties(vec![p_with("weight_kg", |p| {
            p.unit_id = Some(CodedValueId::new("cv-kg-missing"));
        })]);
        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::CodedValue);
    }

    #[test]
    fn unit_id_resolving_inside_an_existing_code_system_passes() {
        let cs = CodeSystemDef {
            id: CodeSystemId::new("cs-ucum"),
            name: "ucum".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes: vec![crate::code_system::CodedValue {
                id: CodedValueId::new("cv-kg"),
                code: "kg".into(),
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
        let mut ir = ontology_with_properties(vec![p_with("weight_kg", |p| {
            p.unit_id = Some(CodedValueId::new("cv-kg"));
        })]);
        ir.add_code_system(cs).expect("code system add");
        assert!(ir.dangling_references().is_empty());
    }
}

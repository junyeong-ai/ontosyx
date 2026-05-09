//! Registry cross-reference integrity.
//!
//! As the ontology grew past a flat node/edge/property model, a
//! thicket of optional id-pointers appeared on `PropertyDef` and
//! elsewhere:
//!
//! - `PropertyBinding::Concept` → `ConceptDef.id`
//! - `PropertyDef.value_set_id` → `ValueSetDef.id`
//! - `PropertyDef.notation_pattern_id` → `NotationPatternDef.id`
//! - `PropertyDef.value_range_set_id` → `ValueRangeSetDef.id`
//! - `PropertyDef.unit_id` → a `CodedValueId` inside some `CodeSystemDef`
//! - `ShaclConstraint::InValueSet` → `ValueSetDef.id`
//! - `ShaclConstraint::MatchesPattern` → `NotationPatternDef.id`
//! - `ValueSetIncludeRule.system_id` → `CodeSystemDef.id`
//! - `ConceptMapDef.source_system_id` / `target_system_id` → `CodeSystemDef.id`
//! - `ConceptMapping.source_code` / `target_code` → concrete `CodedValue.code`
//! - binding `concept_map_id` → `ConceptMapDef.id`
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

use ox_core::diagnostic::{DiagnosticMessage, diag};

use crate::code_system::CodeSystemId;
use crate::code_system::{CodeSystemDef, CodedValueId};
use crate::concept::ConceptId;
use crate::concept_map::{ConceptMapDef, ConceptMapId};
use crate::ir::{EdgeTypeDef, NodeTypeDef, OntologyIR, PropertyDef};
use crate::notation_pattern::NotationPatternId;
#[cfg(test)]
use crate::rule::ConstraintTarget;
use crate::rule::{RuleDef, ShaclConstraint};
use crate::value_range::ValueRangeSetId;
use crate::value_set::{ValueSetDef, ValueSetId, ValueSetSelector};

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
    ConceptMapMapping {
        concept_map_name: String,
        mapping_index: usize,
        field: &'static str,
    },
}

/// The kind of collection the pointer *should* have landed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DanglingKind {
    Concept,
    ValueSet,
    NotationPattern,
    ValueRangeSet,
    CodeSystem,
    CodedValue,
    Code,
    ConceptMap,
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
        let concepts: std::collections::HashSet<&ConceptId> =
            self.concepts().iter().map(|c| &c.id).collect();
        let value_sets: std::collections::HashMap<&ValueSetId, &ValueSetDef> =
            self.value_sets().iter().map(|vs| (&vs.id, vs)).collect();
        let notation_patterns: std::collections::HashSet<&NotationPatternId> =
            self.notation_patterns().iter().map(|np| &np.id).collect();
        let value_ranges: std::collections::HashSet<&ValueRangeSetId> =
            self.value_range_sets().iter().map(|r| &r.id).collect();
        let code_systems: std::collections::HashMap<&CodeSystemId, &CodeSystemDef> =
            self.code_systems().iter().map(|cs| (&cs.id, cs)).collect();
        let concept_maps: std::collections::HashMap<&ConceptMapId, &ConceptMapDef> =
            self.concept_maps().iter().map(|cm| (&cm.id, cm)).collect();

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
                    &concepts,
                    &value_sets,
                    &notation_patterns,
                    &value_ranges,
                    &code_systems,
                    &concept_maps,
                    &mut out,
                );
            }
        }

        // ------------------------------------------------------
        // RuleDef.constraints pointers.
        // ------------------------------------------------------
        for rule in self.rules() {
            rule_pointer_walk(rule, &value_sets, &notation_patterns, &mut out);
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
    concepts: &std::collections::HashSet<&ConceptId>,
    value_sets: &std::collections::HashMap<&ValueSetId, &ValueSetDef>,
    notation_patterns: &std::collections::HashSet<&NotationPatternId>,
    value_ranges: &std::collections::HashSet<&ValueRangeSetId>,
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    concept_maps: &std::collections::HashMap<&ConceptMapId, &ConceptMapDef>,
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
            crate::binding::PropertyBinding::Concept { id, .. } if !concepts.contains(id) => {
                out.push(DanglingReference {
                    source: site("bindings.concept"),
                    kind: DanglingKind::Concept,
                    missing_id: id.to_string(),
                });
            }
            crate::binding::PropertyBinding::ValueSet {
                id, concept_map_id, ..
            } => {
                if !value_sets.contains_key(id) {
                    out.push(DanglingReference {
                        source: site("bindings.value_set"),
                        kind: DanglingKind::ValueSet,
                        missing_id: id.to_string(),
                    });
                }
                check_binding_concept_map(&site, concept_map_id.as_ref(), concept_maps, out);
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
            crate::binding::PropertyBinding::CodeSystem {
                id, concept_map_id, ..
            } => {
                if !code_systems.contains_key(id) {
                    out.push(DanglingReference {
                        source: site("bindings.code_system"),
                        kind: DanglingKind::CodeSystem,
                        missing_id: id.to_string(),
                    });
                }
                check_binding_concept_map(&site, concept_map_id.as_ref(), concept_maps, out);
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

fn check_binding_concept_map(
    site: &impl Fn(&'static str) -> RegistrySite,
    concept_map_id: Option<&ConceptMapId>,
    concept_maps: &std::collections::HashMap<&ConceptMapId, &ConceptMapDef>,
    out: &mut Vec<DanglingReference>,
) {
    if let Some(id) = concept_map_id
        && !concept_maps.contains_key(id)
    {
        out.push(DanglingReference {
            source: site("bindings.concept_map_id"),
            kind: DanglingKind::ConceptMap,
            missing_id: id.to_string(),
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
        | ShaclConstraint::UniqueKey { .. }
        | ShaclConstraint::Or { .. }
        | ShaclConstraint::And { .. }
        | ShaclConstraint::Not { .. }
        | ShaclConstraint::Xone { .. }
        | ShaclConstraint::QualifiedValueShape { .. } => "Other",
    }
}

fn value_set_composition_walk(
    vs: &ValueSetDef,
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    out: &mut Vec<DanglingReference>,
) {
    for (idx, rule) in vs.composition.iter().enumerate() {
        let Some(system) = code_systems.get(&rule.system_id) else {
            out.push(DanglingReference {
                source: RegistrySite::ValueSet {
                    value_set_name: vs.name.clone(),
                    rule_index: idx,
                },
                kind: DanglingKind::CodeSystem,
                missing_id: rule.system_id.to_string(),
            });
            continue;
        };
        match &rule.selector {
            ValueSetSelector::Explicit { codes } => {
                let system_codes: std::collections::HashSet<&str> =
                    system.codes.iter().map(|cv| cv.code.as_str()).collect();
                for code in codes {
                    if !system_codes.contains(code.as_str()) {
                        out.push(DanglingReference {
                            source: RegistrySite::ValueSet {
                                value_set_name: vs.name.clone(),
                                rule_index: idx,
                            },
                            kind: DanglingKind::Code,
                            missing_id: code.clone(),
                        });
                    }
                }
            }
            ValueSetSelector::DescendantsOf { root_id } => {
                if !system.codes.iter().any(|cv| &cv.id == root_id) {
                    out.push(DanglingReference {
                        source: RegistrySite::ValueSet {
                            value_set_name: vs.name.clone(),
                            rule_index: idx,
                        },
                        kind: DanglingKind::CodedValue,
                        missing_id: root_id.to_string(),
                    });
                }
            }
            ValueSetSelector::All | ValueSetSelector::CodePattern { .. } => {}
        }
    }
}

fn concept_map_walk(
    cm: &ConceptMapDef,
    code_systems: &std::collections::HashMap<&CodeSystemId, &CodeSystemDef>,
    out: &mut Vec<DanglingReference>,
) {
    let source_system = code_systems.get(&cm.source_system_id);
    let target_system = code_systems.get(&cm.target_system_id);
    if source_system.is_none() {
        out.push(DanglingReference {
            source: RegistrySite::ConceptMap {
                concept_map_name: cm.name.clone(),
                field: "source_system_id",
            },
            kind: DanglingKind::CodeSystem,
            missing_id: cm.source_system_id.to_string(),
        });
    }
    if target_system.is_none() {
        out.push(DanglingReference {
            source: RegistrySite::ConceptMap {
                concept_map_name: cm.name.clone(),
                field: "target_system_id",
            },
            kind: DanglingKind::CodeSystem,
            missing_id: cm.target_system_id.to_string(),
        });
    }
    let source_codes: Option<std::collections::HashSet<&str>> =
        source_system.map(|system| system.codes.iter().map(|cv| cv.code.as_str()).collect());
    let target_codes: Option<std::collections::HashSet<&str>> =
        target_system.map(|system| system.codes.iter().map(|cv| cv.code.as_str()).collect());
    for (idx, mapping) in cm.mappings.iter().enumerate() {
        if let Some(codes) = &source_codes
            && !codes.contains(mapping.source_code.as_str())
        {
            out.push(DanglingReference {
                source: RegistrySite::ConceptMapMapping {
                    concept_map_name: cm.name.clone(),
                    mapping_index: idx,
                    field: "source_code",
                },
                kind: DanglingKind::Code,
                missing_id: mapping.source_code.clone(),
            });
        }
        if let Some(codes) = &target_codes
            && !codes.contains(mapping.target_code.as_str())
        {
            out.push(DanglingReference {
                source: RegistrySite::ConceptMapMapping {
                    concept_map_name: cm.name.clone(),
                    mapping_index: idx,
                    field: "target_code",
                },
                kind: DanglingKind::Code,
                missing_id: mapping.target_code.clone(),
            });
        }
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
                DanglingKind::Concept => "concept",
                DanglingKind::ValueSet => "value_set",
                DanglingKind::NotationPattern => "notation_pattern",
                DanglingKind::ValueRangeSet => "value_range_set",
                DanglingKind::CodeSystem => "code_system",
                DanglingKind::CodedValue => "coded_value",
                DanglingKind::Code => "code",
                DanglingKind::ConceptMap => "concept_map",
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
                RegistrySite::ConceptMapMapping {
                    concept_map_name,
                    mapping_index,
                    field,
                } => (
                    format!(
                        "concept map `{concept_map_name}` mapping {mapping_index} field `{field}`"
                    ),
                    diag(format!("ontology.validate.integrity.dangling_{kind}"))
                        .with("site_kind", "concept_map_mapping")
                        .with("concept_map_name", concept_map_name.clone())
                        .with("mapping_index", *mapping_index as u64)
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
    use crate::action::RuleId;
    use crate::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue};
    use crate::concept::ConceptId;
    use crate::concept_map::{ConceptMapId, ConceptMapping, Equivalence};
    use crate::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId};
    use crate::rule::{
        EnforcementKind, RuleActivationKind, RuleDef, RuleKind, RuleOrigin, Severity,
        ShaclConstraint,
    };
    use crate::value_set::{IncludeMode, ValueSetId, ValueSetIncludeRule, ValueSetSelector};
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

    fn cv(id: &str, code: &str, broader_id: Option<&str>) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: code.into(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: vec![],
            broader_id: broader_id.map(CodedValueId::new),
            examples: vec![],
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn code_system(id: &str, hierarchical: bool, codes: Vec<CodedValue>) -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new(id),
            name: id.into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical,
            codes,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    #[test]
    fn empty_ontology_has_no_dangling_refs() {
        let ir = ontology_with_properties(vec![p_with("name", |_| {})]);
        assert!(ir.dangling_references().is_empty());
    }

    #[test]
    fn property_with_missing_concept_surfaces() {
        let ir = ontology_with_properties(vec![p_with("name", |p| {
            p.bindings
                .push(crate::binding::PropertyBinding::concept(ConceptId::new(
                    "c-missing",
                )));
        })]);
        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::Concept);
        assert_eq!(refs[0].missing_id, "c-missing");
    }

    #[test]
    fn property_with_missing_value_set_surfaces() {
        let ir = ontology_with_properties(vec![p_with("status", |p| {
            p.bindings
                .push(crate::binding::PropertyBinding::value_set(ValueSetId::new(
                    "vs-missing",
                )));
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
        let cs = code_system("cs-ucum", false, vec![cv("cv-kg", "kg", None)]);
        let mut ir = ontology_with_properties(vec![p_with("weight_kg", |p| {
            p.unit_id = Some(CodedValueId::new("cv-kg"));
        })]);
        ir.add_code_system(cs).expect("code system add");
        assert!(ir.dangling_references().is_empty());
    }

    #[test]
    fn explicit_value_set_code_missing_from_system_surfaces() {
        let mut ir = ontology_with_properties(vec![]);
        ir.add_code_system(code_system(
            "cs-status",
            false,
            vec![cv("cv-active", "ACTIVE", None)],
        ))
        .expect("code system add");
        ir.value_sets.push(ValueSetDef {
            id: ValueSetId::new("vs-status"),
            name: "Status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("cs-status"),
                selector: ValueSetSelector::Explicit {
                    codes: vec!["ACTIVE".into(), "MISSING".into()],
                },
                mode: IncludeMode::Include,
            }],
        });
        ir.rebuild_indices().expect("index rebuild");

        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::Code);
        assert_eq!(refs[0].missing_id, "MISSING");
        assert!(matches!(&refs[0].source, RegistrySite::ValueSet { .. }));
    }

    #[test]
    fn descendants_value_set_missing_root_surfaces() {
        let mut ir = ontology_with_properties(vec![]);
        ir.add_code_system(code_system(
            "cs-topic",
            true,
            vec![cv("cv-root", "ROOT", None)],
        ))
        .expect("code system add");
        ir.value_sets.push(ValueSetDef {
            id: ValueSetId::new("vs-topic"),
            name: "TopicDescendants".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("cs-topic"),
                selector: ValueSetSelector::DescendantsOf {
                    root_id: CodedValueId::new("cv-missing-root"),
                },
                mode: IncludeMode::Include,
            }],
        });
        ir.rebuild_indices().expect("index rebuild");

        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::CodedValue);
        assert_eq!(refs[0].missing_id, "cv-missing-root");
    }

    #[test]
    fn concept_map_mapping_code_missing_from_declared_system_surfaces() {
        let mut ir = ontology_with_properties(vec![]);
        ir.add_code_system(code_system(
            "cs-source",
            false,
            vec![cv("cv-active", "ACTIVE", None)],
        ))
        .expect("source system add");
        ir.add_code_system(code_system("cs-target", false, vec![cv("cv-a", "A", None)]))
            .expect("target system add");
        ir.concept_maps.push(ConceptMapDef {
            id: ConceptMapId::new("cm-status"),
            name: "StatusMap".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            source_system_id: CodeSystemId::new("cs-source"),
            target_system_id: CodeSystemId::new("cs-target"),
            mappings: vec![ConceptMapping {
                source_code: "MISSING_SOURCE".into(),
                target_code: "MISSING_TARGET".into(),
                equivalence: Equivalence::Equivalent,
                comment: LocalizedText::default(),
            }],
        });
        ir.rebuild_indices().expect("index rebuild");

        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 2);
        assert!(refs.iter().any(|r| {
            r.kind == DanglingKind::Code
                && r.missing_id == "MISSING_SOURCE"
                && matches!(
                    &r.source,
                    RegistrySite::ConceptMapMapping {
                        field: "source_code",
                        ..
                    }
                )
        }));
        assert!(refs.iter().any(|r| {
            r.kind == DanglingKind::Code
                && r.missing_id == "MISSING_TARGET"
                && matches!(
                    &r.source,
                    RegistrySite::ConceptMapMapping {
                        field: "target_code",
                        ..
                    }
                )
        }));
    }

    #[test]
    fn binding_with_missing_concept_map_surfaces() {
        let mut ir = ontology_with_properties(vec![p_with("status", |p| {
            p.bindings.push(
                crate::binding::PropertyBinding::value_set(ValueSetId::new("vs-status"))
                    .with_concept_map(ConceptMapId::new("cm-missing")),
            );
        })]);
        ir.add_code_system(code_system(
            "cs-status",
            false,
            vec![cv("cv-active", "ACTIVE", None)],
        ))
        .expect("code system add");
        ir.add_value_set(ValueSetDef {
            id: ValueSetId::new("vs-status"),
            name: "Status".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("cs-status"),
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            }],
        })
        .expect("value set add");

        let refs = ir.dangling_references();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, DanglingKind::ConceptMap);
        assert_eq!(refs[0].missing_id, "cm-missing");
    }
}

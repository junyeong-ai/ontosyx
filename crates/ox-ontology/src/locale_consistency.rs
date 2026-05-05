//! Multi-locale consistency checks.
//!
//! The platform's user-facing strings (`display_name`, `description`,
//! `business_context`, `aliases`) are `LocalizedText` values — a
//! canonical default plus per-locale translations. Nothing in the
//! save path requires a translation to exist for every locale, which
//! is the right default (an early-stage ontology can ship with just
//! Korean; English translations arrive incrementally).
//!
//! The cost shows up on the reporting side: once an operator claims
//! "this workspace speaks `ko` + `en`", any `display_name` that
//! forgot its English translation becomes a silent gap — the UI
//! falls back to the canonical default, which is almost always
//! Korean, and English users see Korean text on a localized page.
//!
//! This module walks every localizable surface, compares it against
//! a caller-declared `required_locales` set, and emits a
//! `LocaleGap` for every missing translation. The result is
//! informational — gaps do not block validation — but surfaces in
//! the Quality Signals dashboard as a point-in-time "how
//! localization-complete is this ontology?" measurement.

use ox_core::i18n::{LanguageTag, LocalizedText};

use crate::ir::{NodeTypeDef, OntologyIR};

/// One missing translation. `subject` / `subject_id` identify the
/// entity carrying the gap so the UI can link to its editor; `field`
/// picks out the specific field on that entity; `missing_locales`
/// lists every required locale that has no (non-empty) translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleGap {
    pub subject: LocaleSubject,
    pub subject_id: String,
    pub field: &'static str,
    pub missing_locales: Vec<String>,
}

/// Which kind of entity the gap belongs to. Kept as a separate enum
/// (rather than a free-form string) so the admin UI can route each
/// subject to the right editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleSubject {
    NodeType,
    EdgeType,
    NodeProperty { owner_label: String },
    EdgeProperty { owner_label: String },
    GlossaryTerm,
    CodeSystem,
    CodedValue { code_system_id: String },
    ValueSet,
    NotationPattern,
}

/// Walk the ontology and emit one `LocaleGap` for every localizable
/// field that is missing at least one required-locale translation.
/// Fields are considered "present" when they carry a non-empty
/// value for the locale — an empty string counts as missing.
pub fn detect_locale_gaps(
    ontology: &OntologyIR,
    required_locales: &[LanguageTag],
) -> Vec<LocaleGap> {
    let mut out = Vec::new();
    if required_locales.is_empty() {
        return out;
    }

    for node in ontology.node_types() {
        check(
            &node.description,
            LocaleSubject::NodeType,
            node.id.to_string(),
            "description",
            required_locales,
            &mut out,
        );
        for p in &node.properties {
            node_property_checks(node, p, required_locales, &mut out);
        }
    }
    for edge in ontology.edge_types() {
        check(
            &edge.description,
            LocaleSubject::EdgeType,
            edge.id.to_string(),
            "description",
            required_locales,
            &mut out,
        );
        for p in &edge.properties {
            check(
                &p.display_name,
                LocaleSubject::EdgeProperty {
                    owner_label: edge.label.as_str().to_string(),
                },
                p.id.to_string(),
                "display_name",
                required_locales,
                &mut out,
            );
            check(
                &p.description,
                LocaleSubject::EdgeProperty {
                    owner_label: edge.label.as_str().to_string(),
                },
                p.id.to_string(),
                "description",
                required_locales,
                &mut out,
            );
        }
    }

    for term in ontology.glossary() {
        check(
            &term.term,
            LocaleSubject::GlossaryTerm,
            term.id.to_string(),
            "term",
            required_locales,
            &mut out,
        );
        check(
            &term.display_name,
            LocaleSubject::GlossaryTerm,
            term.id.to_string(),
            "display_name",
            required_locales,
            &mut out,
        );
        check(
            &term.description,
            LocaleSubject::GlossaryTerm,
            term.id.to_string(),
            "description",
            required_locales,
            &mut out,
        );
    }
    for cs in ontology.code_systems() {
        check(
            &cs.display_name,
            LocaleSubject::CodeSystem,
            cs.id.to_string(),
            "display_name",
            required_locales,
            &mut out,
        );
        for cv in &cs.codes {
            check(
                &cv.display,
                LocaleSubject::CodedValue {
                    code_system_id: cs.id.to_string(),
                },
                cv.id.to_string(),
                "display",
                required_locales,
                &mut out,
            );
        }
    }
    for vs in ontology.value_sets() {
        check(
            &vs.display_name,
            LocaleSubject::ValueSet,
            vs.id.to_string(),
            "display_name",
            required_locales,
            &mut out,
        );
    }
    for np in ontology.notation_patterns() {
        check(
            &np.display_name,
            LocaleSubject::NotationPattern,
            np.id.to_string(),
            "display_name",
            required_locales,
            &mut out,
        );
    }

    out
}

fn node_property_checks(
    node: &NodeTypeDef,
    property: &crate::ir::PropertyDef,
    required: &[LanguageTag],
    out: &mut Vec<LocaleGap>,
) {
    let owner = || LocaleSubject::NodeProperty {
        owner_label: node.label.as_str().to_string(),
    };
    check(
        &property.display_name,
        owner(),
        property.id.to_string(),
        "display_name",
        required,
        out,
    );
    check(
        &property.description,
        owner(),
        property.id.to_string(),
        "description",
        required,
        out,
    );
}

/// If the localized value is empty entirely, we skip it — that's a
/// "field wasn't filled in" state, not a "missing translation"
/// state, and surfacing it here would flood the report. The primary
/// `OntologyIR::validate()` pass already flags missing required
/// fields. What we want to catch is the "canonical default exists
/// but translation is missing" case: when `default` is non-empty,
/// every required locale must also be non-empty.
fn check(
    text: &LocalizedText,
    subject: LocaleSubject,
    subject_id: String,
    field: &'static str,
    required: &[LanguageTag],
    out: &mut Vec<LocaleGap>,
) {
    if text.default_str().trim().is_empty() {
        return;
    }
    // `resolve(&[tag])` falls back to `default` when the translation
    // is absent, so we can't use it to detect missing translations.
    // Walk the iter directly — `iter()` yields `(None, default)` then
    // `(Some(tag), translation)` pairs; we want a non-empty match on
    // the requested tag specifically.
    let missing: Vec<String> = required
        .iter()
        .filter(|tag| {
            !text
                .iter()
                .any(|(t, v)| t == Some(*tag) && !v.trim().is_empty())
        })
        .map(|tag| tag.as_str().to_string())
        .collect();
    if missing.is_empty() {
        return;
    }
    out.push(LocaleGap {
        subject,
        subject_id,
        field,
        missing_locales: missing,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glossary::{GlossaryTermDef, GlossaryTermId};
    use crate::ir::{NodeTypeDef, NodeTypeId, OntologyIR};
    use ox_core::graph_label::GraphLabel;

    fn ontology_with_node(description: LocalizedText) -> OntologyIR {
        OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt"),
                label: GraphLabel::new("N").unwrap(),
                description,
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid seed ontology")
    }

    #[test]
    fn empty_required_locales_yields_no_gaps() {
        let ir = ontology_with_node(LocalizedText::new("hello"));
        assert!(detect_locale_gaps(&ir, &[]).is_empty());
    }

    #[test]
    fn untranslated_field_surfaces_for_each_required_locale() {
        let ir = ontology_with_node(LocalizedText::new("안녕"));
        let gaps = detect_locale_gaps(&ir, &[LanguageTag::parse("en").unwrap()]);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].missing_locales, vec!["en".to_string()]);
    }

    #[test]
    fn fully_translated_field_yields_no_gap() {
        let mut ir = ontology_with_node(LocalizedText::default());
        let node = &mut ir.node_types_mut()[0];
        node.description = LocalizedText::new("안녕").with_translation(
            LanguageTag::parse("en").unwrap(),
            "hello",
        );
        let gaps = detect_locale_gaps(&ir, &[LanguageTag::parse("en").unwrap()]);
        assert!(gaps.is_empty(), "{gaps:?}");
    }

    #[test]
    fn empty_default_is_ignored_as_not_filled_in() {
        // Empty default is a "not filled in" state, not a translation
        // gap — the primary validate() pass catches the former, this
        // pass should stay silent.
        let ir = ontology_with_node(LocalizedText::default());
        let gaps = detect_locale_gaps(&ir, &[LanguageTag::parse("en").unwrap()]);
        assert!(gaps.is_empty());
    }

    #[test]
    fn glossary_term_gaps_surface() {
        let mut ir = OntologyIR::try_new(
            "ont".into(),
            "Test".into(),
            LocalizedText::default(),
            1u32,
            vec![NodeTypeDef {
                id: NodeTypeId::new("nt"),
                label: GraphLabel::new("N").unwrap(),
                ..Default::default()
            }],
            vec![],
            vec![],
        )
        .expect("valid seed ontology");
        ir.add_glossary_term(GlossaryTermDef {
            id: GlossaryTermId::new("g1"),
            term: LocalizedText::new("customer_grade"),
            display_name: LocalizedText::new("고객 등급"),
            description: LocalizedText::new("고객 등급 분류"),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
        concept_id: None,
        })
        .unwrap();
        let gaps = detect_locale_gaps(&ir, &[LanguageTag::parse("en").unwrap()]);
        // term + display_name + description each miss `en` → 3 gaps.
        assert_eq!(gaps.len(), 3);
        for g in &gaps {
            assert!(matches!(g.subject, LocaleSubject::GlossaryTerm));
        }
    }
}

//! SKOS Turtle export for the workspace glossary.
//!
//! Produces a Concept Scheme + Concept tree alignable with W3C SKOS
//! Core (https://www.w3.org/TR/skos-reference/) so the catalogue can
//! be round-tripped to Collibra / Atlan / Eurovoc / public sector
//! thesauri without parsing prose.
//!
//! Mapping:
//! - Workspace glossary    → `skos:ConceptScheme` (one per ontology)
//! - `GlossaryTermDef`     → `skos:Concept`
//! - `term` (LocalizedText) → `skos:prefLabel` (one per locale)
//! - `display_name`         → `skos:altLabel` (one per locale)
//! - `aliases`              → `skos:altLabel` (one per alias × locale)
//! - `description`          → `skos:definition`
//! - `examples`             → `skos:example`
//! - `TermRelation::Broader/Narrower/Related/SeeAlso/ExactMatch/CloseMatch`
//!   → `skos:broader/narrower/related/seeAlso/exactMatch/closeMatch`
//! - `governance.scope_notes`     → `skos:scopeNote`
//! - `governance.editorial_notes` → `skos:editorialNote`
//! - `governance.change_notes`    → `skos:changeNote` (one per entry)
//! - `lifecycle: Deprecated/Retired` → `owl:deprecated true` plus a
//!   `dcterms:isReplacedBy` triple when a successor is named.
//!
//! Routine-deferred SKOS-XL features (per-label reification with
//! their own metadata) are intentionally not emitted: the simpler
//! `skos:prefLabel` literal form is what most catalogues consume.

use ox_core::i18n::LocalizedText;
use ox_ontology::glossary::{TermLifecycle, TermRelationKind};
use ox_ontology::ir::OntologyIR;

/// Render the workspace's glossary as a SKOS Turtle document. Returns
/// the empty string when the glossary is empty so the caller can
/// short-circuit a "nothing to export" UI without juggling Options.
pub fn generate_glossary_skos(ontology: &OntologyIR) -> String {
    if ontology.glossary().is_empty() {
        return String::new();
    }

    let base_ns = format!(
        "http://ontosyx.io/ontology/{}/glossary",
        uri_encode(&ontology.name),
    );

    let mut out = String::new();

    // ---- Prefixes ------------------------------------------------
    out.push_str("@prefix skos:    <http://www.w3.org/2004/02/skos/core#> .\n");
    out.push_str("@prefix owl:     <http://www.w3.org/2002/07/owl#> .\n");
    out.push_str("@prefix dcterms: <http://purl.org/dc/terms/> .\n");
    out.push_str("@prefix xsd:     <http://www.w3.org/2001/XMLSchema#> .\n");
    out.push_str(&format!("@prefix :        <{base_ns}#> .\n"));
    out.push('\n');

    // ---- ConceptScheme -------------------------------------------
    out.push_str(&format!("<{base_ns}> a skos:ConceptScheme ;\n"));
    out.push_str(&format!(
        "    dcterms:title {} .\n\n",
        turtle_literal(&format!("{} glossary", ontology.name)),
    ));

    // ---- Concepts ------------------------------------------------
    for term in ontology.glossary() {
        let concept_id = local_name(term.id.as_str());
        out.push_str(&format!(":{concept_id} a skos:Concept ;\n"));
        out.push_str(&format!(
            "    skos:inScheme <{base_ns}> ;\n",
            base_ns = base_ns,
        ));

        // prefLabel — one per locale, taking the term's `default`
        // as the untagged form and each translation as a tagged
        // literal. SKOS conformance lets a Concept have multiple
        // prefLabels iff each carries a distinct language tag.
        write_localized_labels(&mut out, "skos:prefLabel", &term.term);
        write_localized_labels(&mut out, "skos:altLabel", &term.display_name);
        for alias in &term.aliases {
            write_localized_labels(&mut out, "skos:altLabel", alias);
        }

        write_localized_labels(&mut out, "skos:definition", &term.description);
        for example in &term.examples {
            write_localized_labels(&mut out, "skos:example", example);
        }

        for note in &term.governance.scope_notes {
            write_localized_labels(&mut out, "skos:scopeNote", note);
        }
        for note in &term.governance.editorial_notes {
            write_localized_labels(&mut out, "skos:editorialNote", note);
        }
        for note in &term.governance.change_notes {
            // Change notes are timestamped; encode the timestamp as
            // a parenthetical prefix on the localised note body so
            // round-trip preserves "when".
            let with_ts = LocalizedText::new(format!("({}) {}", note.at, note.note.default));
            write_localized_labels(&mut out, "skos:changeNote", &with_ts);
        }

        for relation in &term.related_terms {
            let predicate = match relation.kind {
                TermRelationKind::Broader => "skos:broader",
                TermRelationKind::Narrower => "skos:narrower",
                TermRelationKind::Related => "skos:related",
                TermRelationKind::SeeAlso => "rdfs:seeAlso",
                TermRelationKind::ExactMatch => "skos:exactMatch",
                TermRelationKind::CloseMatch => "skos:closeMatch",
            };
            out.push_str(&format!(
                "    {predicate} :{} ;\n",
                local_name(relation.target.as_str()),
            ));
        }

        match &term.lifecycle {
            TermLifecycle::Active => {}
            TermLifecycle::Deprecated {
                replaced_by,
                deprecated_at,
            } => {
                out.push_str("    owl:deprecated true ;\n");
                out.push_str(&format!(
                    "    dcterms:date \"{deprecated_at}\"^^xsd:dateTime ;\n"
                ));
                if let Some(target) = replaced_by {
                    out.push_str(&format!(
                        "    dcterms:isReplacedBy :{} ;\n",
                        local_name(target.as_str()),
                    ));
                }
            }
            TermLifecycle::Retired { retired_at } => {
                out.push_str("    owl:deprecated true ;\n");
                out.push_str(&format!(
                    "    dcterms:dateRetired \"{retired_at}\"^^xsd:dateTime ;\n"
                ));
            }
        }

        // Strip the dangling " ;\n" off the last triple line and
        // close the concept with " .\n\n" so the document parses.
        finalise_block(&mut out);
        out.push('\n');
    }

    // ---- Type-level realisations --------------------------------
    // NodeType / EdgeType `glossary_anchors` lift the `Concept ↔
    // Class/Property` SKOS link to the type tier so a downstream
    // catalogue can navigate from a business concept directly to
    // every concrete type that realises it. Emit each anchor as a
    // `skos:exactMatch` between the type's URI and the concept,
    // which is what TopBraid EDG / Stardog Designer consume when
    // they overlay glossary onto class diagrams.
    let type_ns = format!(
        "http://ontosyx.io/ontology/{}",
        uri_encode(&ontology.name),
    );
    let mut wrote_any_realisation = false;
    for node in ontology.node_types() {
        if node.glossary_anchors.is_empty() {
            continue;
        }
        if !wrote_any_realisation {
            out.push_str("# ---- Type realisations ----\n");
            wrote_any_realisation = true;
        }
        out.push_str(&format!(
            "<{type_ns}/node/{}>\n",
            uri_encode(node.label.as_str()),
        ));
        for term_id in &node.glossary_anchors {
            out.push_str(&format!(
                "    skos:exactMatch :{} ;\n",
                local_name(term_id.as_str()),
            ));
        }
        finalise_block(&mut out);
        out.push('\n');
    }
    for edge in ontology.edge_types() {
        if edge.glossary_anchors.is_empty() {
            continue;
        }
        if !wrote_any_realisation {
            out.push_str("# ---- Type realisations ----\n");
            wrote_any_realisation = true;
        }
        out.push_str(&format!(
            "<{type_ns}/edge/{}>\n",
            uri_encode(edge.label.as_str()),
        ));
        for term_id in &edge.glossary_anchors {
            out.push_str(&format!(
                "    skos:exactMatch :{} ;\n",
                local_name(term_id.as_str()),
            ));
        }
        finalise_block(&mut out);
        out.push('\n');
    }

    out
}

/// Emit every locale variant of a `LocalizedText` as a separate
/// `<predicate> "value"@<lang> ;` triple, plus an untagged literal
/// for the `default` value (the SKOS-recommended fallback for
/// catalogue consumers that ignore language tags).
fn write_localized_labels(out: &mut String, predicate: &str, text: &LocalizedText) {
    if !text.default.trim().is_empty() {
        out.push_str(&format!(
            "    {predicate} {} ;\n",
            turtle_literal(&text.default),
        ));
    }
    for (tag, value) in &text.translations {
        if value.trim().is_empty() {
            continue;
        }
        out.push_str(&format!(
            "    {predicate} {}@{} ;\n",
            turtle_literal(value),
            tag.as_str(),
        ));
    }
}

/// Replace the trailing " ;\n" with " .\n" so the current concept's
/// triple block terminates correctly. Called after the last triple
/// of every concept; safe no-op when the buffer doesn't end in a
/// pending separator (empty term).
fn finalise_block(out: &mut String) {
    if out.ends_with(" ;\n") {
        let len = out.len();
        out.truncate(len - 3);
        out.push_str(" .\n");
    }
}

fn turtle_literal(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

fn local_name(s: &str) -> String {
    let mut name: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if name.is_empty() {
        name.push_str("_unnamed");
    }
    name
}

fn uri_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_ontology::glossary::{
        GlossaryTermDef, GlossaryTermId, TermGovernance, TermLifecycle,
    };
    use ox_core::i18n::{LanguageTag, LocalizedText};

    fn empty_term(id: &str, label: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(label),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::Active,
        }
    }

    fn ontology_with(terms: Vec<GlossaryTermDef>) -> OntologyIR {
        let mut onto = OntologyIR::new(
            "test-onto".to_string(),
            "TestOnto".to_string(),
            LocalizedText::default(),
            1,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        for t in terms {
            onto.add_glossary_term(t).expect("add glossary term");
        }
        onto
    }

    #[test]
    fn empty_glossary_renders_empty_string() {
        let onto = ontology_with(Vec::new());
        assert!(generate_glossary_skos(&onto).is_empty());
    }

    #[test]
    fn single_term_renders_skos_concept_scheme_plus_concept() {
        let onto = ontology_with(vec![empty_term("gt-customer", "Customer")]);
        let ttl = generate_glossary_skos(&onto);
        assert!(ttl.contains("a skos:ConceptScheme"));
        assert!(ttl.contains(":gt_customer a skos:Concept"));
        assert!(ttl.contains("skos:prefLabel \"Customer\""));
    }

    #[test]
    fn term_with_translations_emits_per_locale_pref_labels() {
        let mut t = empty_term("gt-customer", "Customer");
        t.term =
            LocalizedText::new("Customer").with_translation(LanguageTag::ko(), "고객");
        let onto = ontology_with(vec![t]);
        let ttl = generate_glossary_skos(&onto);
        assert!(ttl.contains("skos:prefLabel \"Customer\""));
        assert!(ttl.contains("skos:prefLabel \"고객\"@ko"));
    }

    #[test]
    fn deprecated_term_emits_owl_deprecated_and_replaced_by() {
        use chrono::Utc;
        let onto = ontology_with(vec![
            empty_term("gt-new", "Customer"),
            GlossaryTermDef {
                lifecycle: TermLifecycle::Deprecated {
                    replaced_by: Some(GlossaryTermId::new("gt-new")),
                    deprecated_at: Utc::now(),
                },
                ..empty_term("gt-old", "Client")
            },
        ]);
        let ttl = generate_glossary_skos(&onto);
        assert!(ttl.contains("owl:deprecated true"));
        assert!(ttl.contains("dcterms:isReplacedBy :gt_new"));
    }

    #[test]
    fn aliases_emit_alt_labels_per_locale() {
        let mut t = empty_term("gt-customer", "Customer");
        t.aliases = vec![
            LocalizedText::new("Buyer").with_translation(LanguageTag::ko(), "구매자"),
        ];
        let onto = ontology_with(vec![t]);
        let ttl = generate_glossary_skos(&onto);
        assert!(ttl.contains("skos:altLabel \"Buyer\""));
        assert!(ttl.contains("skos:altLabel \"구매자\"@ko"));
    }
}

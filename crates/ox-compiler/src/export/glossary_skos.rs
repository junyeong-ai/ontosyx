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
        let term_local_id = local_name(term.id.as_str());
        out.push_str(&format!(":{term_local_id} a skos:Concept ;\n"));
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

    // ---- Concept identity layer ---------------------------------
    // `ConceptDef` is the workspace-canonical identity above any
    // single lexicalization. Distinct from the GlossaryTerm
    // export above (which captures every lexicalization the
    // catalogue carries) — the concept emits as its own
    // `skos:Concept` whose `prefLabel` is *resolved through* the
    // canonical term, so a SKOS / TopBraid consumer sees the
    // identity layer directly without re-deriving the link.
    if !ontology.concepts().is_empty() {
        // Build a quick `term_id → &GlossaryTermDef` index so the
        // resolver below is O(1) per concept rather than O(N²)
        // across the term × concept cross product.
        use std::collections::HashMap;
        let term_index: HashMap<&str, &ox_ontology::glossary::GlossaryTermDef> = ontology
            .glossary()
            .iter()
            .map(|t| (t.id.as_str(), t))
            .collect();

        out.push_str("# ---- Concept identity layer ----\n");
        for concept in ontology.concepts() {
            let concept_local = local_name(concept.id.as_str());
            out.push_str(&format!(":{concept_local} a skos:Concept ;\n",));
            out.push_str(&format!(
                "    skos:inScheme <{base_ns}> ;\n",
                base_ns = base_ns,
            ));

            // prefLabel resolved through the canonical
            // GlossaryTerm — operators reading the SKOS export
            // see the same canonical short form they typed in
            // the FE without inventing a second source of
            // truth.
            if let Some(term) = term_index.get(concept.canonical_term_id.as_str()) {
                write_localized_labels(&mut out, "skos:prefLabel", &term.term);
            }
            // altLabel for every alias term — same lexical
            // resolution. The `&**id` deref pierces the newtype
            // wrapper to reach `String::as_str` unambiguously
            // (the newtype's own `as_str()` method conflicts
            // with the `Deref<Target=String>::as_str` blanket
            // — explicit deref keeps inference happy).
            for alias_id in &concept.alias_term_ids {
                let key: &str = alias_id;
                if let Some(term) = term_index.get(key) {
                    write_localized_labels(&mut out, "skos:altLabel", &term.term);
                }
            }

            // skos:broader for the concept hierarchy parent.
            // Points at another `:concept_X` URI on the same
            // ConceptScheme — the standard SKOS narrower /
            // broader walk.
            if let Some(parent) = &concept.broader {
                let parent_str: &str = parent;
                out.push_str(&format!("    skos:broader :{} ;\n", local_name(parent_str),));
            }

            // Concept-level description + examples — distinct
            // from the canonical-term's prose because the
            // ConceptDef carries identity-stable definition
            // text the catalogue prefers over a translation-
            // shaped lexical record.
            write_localized_labels(&mut out, "skos:definition", &concept.description);
            for example in &concept.examples {
                write_localized_labels(&mut out, "skos:example", example);
            }

            // Lifecycle — same deprecate / retire shape the
            // GlossaryTerm export uses, but anchored on the
            // concept identity. SKOS consumers chasing
            // dcterms:isReplacedBy through concept-graph land
            // on the right successor regardless of which
            // lexicalization was renamed.
            match &concept.lifecycle {
                TermLifecycle::Active => {}
                TermLifecycle::Deprecated {
                    replaced_by: _,
                    deprecated_at,
                } => {
                    out.push_str("    owl:deprecated true ;\n");
                    out.push_str(&format!(
                        "    dcterms:date \"{deprecated_at}\"^^xsd:dateTime ;\n"
                    ));
                    if let Some(successor) = &concept.replaced_by {
                        let successor_str: &str = successor;
                        out.push_str(&format!(
                            "    dcterms:isReplacedBy :{} ;\n",
                            local_name(successor_str),
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

            finalise_block(&mut out);
            out.push('\n');
        }
    }

    // ---- Type-level realisations --------------------------------
    // NodeType / EdgeType concept realisations lift the concept link
    // to the type tier so a downstream catalogue can navigate from a
    // business concept directly to every concrete type that realises it.
    let type_ns = format!("http://ontosyx.io/ontology/{}", uri_encode(&ontology.name),);
    let mut wrote_any_realisation = false;
    for node in ontology.node_types() {
        let mut concept_ids: Vec<&ox_ontology::concept::ConceptId> = Vec::new();
        if let Some(concept_id) = &node.concept_id {
            concept_ids.push(concept_id);
        }
        concept_ids.extend(node.concept_realizations.iter().map(|r| &r.concept_id));
        if concept_ids.is_empty() {
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
        for concept_id in concept_ids {
            out.push_str(&format!(
                "    skos:exactMatch :{} ;\n",
                local_name(concept_id.as_str()),
            ));
        }
        finalise_block(&mut out);
        out.push('\n');
    }
    for edge in ontology.edge_types() {
        let mut concept_ids: Vec<&ox_ontology::concept::ConceptId> = Vec::new();
        if let Some(concept_id) = &edge.concept_id {
            concept_ids.push(concept_id);
        }
        concept_ids.extend(edge.concept_realizations.iter().map(|r| &r.concept_id));
        if concept_ids.is_empty() {
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
        for concept_id in concept_ids {
            out.push_str(&format!(
                "    skos:exactMatch :{} ;\n",
                local_name(concept_id.as_str()),
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
    use ox_core::i18n::{LanguageTag, LocalizedText};
    use ox_ontology::glossary::{GlossaryTermDef, GlossaryTermId, TermGovernance, TermLifecycle};

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
            concept_id: None,
            term_pos: Default::default(),
        }
    }

    fn concept_term(
        id: &str,
        label: &str,
        concept_id: &ox_ontology::concept::ConceptId,
    ) -> GlossaryTermDef {
        GlossaryTermDef {
            concept_id: Some(concept_id.clone()),
            ..empty_term(id, label)
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
        t.term = LocalizedText::new("Customer").with_translation(LanguageTag::ko(), "고객");
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
        t.aliases = vec![LocalizedText::new("Buyer").with_translation(LanguageTag::ko(), "구매자")];
        let onto = ontology_with(vec![t]);
        let ttl = generate_glossary_skos(&onto);
        assert!(ttl.contains("skos:altLabel \"Buyer\""));
        assert!(ttl.contains("skos:altLabel \"구매자\"@ko"));
    }

    #[test]
    fn concept_emits_identity_layer_with_resolved_pref_label() {
        // Concept identity layer rides alongside the
        // GlossaryTerm export. The concept's `prefLabel`
        // resolves through `canonical_term_id` so the SKOS
        // consumer reads the canonical short form without
        // re-deriving the link. `skos:broader` lands when a
        // parent concept is set; `skos:altLabel` rolls up
        // every alias term's `term` literal.
        use ox_ontology::concept::{ConceptDef, ConceptGovernance, ConceptId};

        let party_id = ConceptId::new("c-party");
        let customer_id = ConceptId::new("c-customer");
        let party = concept_term("gt-party", "Party", &party_id);
        let canonical = concept_term("gt-customer", "Customer", &customer_id);
        let alias = concept_term("gt-buyer", "Buyer", &customer_id);
        let mut onto = ontology_with(vec![party, canonical, alias]);
        // Parent concept first so the broader pointer resolves.
        onto.add_concept(ConceptDef {
            id: party_id.clone(),
            canonical_term_id: GlossaryTermId::new("gt-party"),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::new("Generic party"),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: TermLifecycle::Active,
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: ConceptGovernance::default(),
        })
        .expect("add c-party");
        onto.add_concept(ConceptDef {
            id: customer_id,
            canonical_term_id: GlossaryTermId::new("gt-customer"),
            alias_term_ids: vec![GlossaryTermId::new("gt-buyer")],
            broader: Some(party_id),
            description: LocalizedText::new("Buyer side of every order"),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: TermLifecycle::Active,
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: ConceptGovernance::default(),
        })
        .expect("add c-customer");

        let ttl = generate_glossary_skos(&onto);
        // Concept identity block emits with its own URI.
        assert!(
            ttl.contains(":c_customer a skos:Concept"),
            "concept identity block missing: {ttl}",
        );
        // prefLabel resolved through gt-customer.
        assert!(
            ttl.contains("skos:prefLabel \"Customer\""),
            "concept prefLabel via canonical term missing: {ttl}",
        );
        // altLabel resolved through gt-buyer.
        assert!(
            ttl.contains("skos:altLabel \"Buyer\""),
            "concept altLabel via alias term missing: {ttl}",
        );
        // Broader pointer.
        assert!(
            ttl.contains("skos:broader :c_party"),
            "concept broader missing: {ttl}",
        );
        // Concept-level definition (distinct from the term's
        // empty description).
        assert!(
            ttl.contains("skos:definition \"Buyer side of every order\""),
            "concept definition missing: {ttl}",
        );
    }
}

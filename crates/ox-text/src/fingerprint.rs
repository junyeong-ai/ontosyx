//! Glossary tokenizer fingerprint.
//!
//! sha256 over **tokenizer-relevant** glossary state — the
//! diff signal `commit_version` reads to decide whether the
//! workspace user dict needs rebuilding. Metadata that doesn't
//! affect tokenization (description, examples, audit
//! timestamps, governance) is intentionally omitted so a doc
//! polish doesn't churn the dict.
//!
//! Tokenizer-relevant fields:
//! - term id (canon order anchor)
//! - term surface (default + every translation)
//! - alias surfaces (default + every translation)
//! - term_pos (drives mecab tag emission)
//! - lifecycle (Active vs Deprecated/Retired affects emission)
//! - concept_id + Concept.canonical_term_id (drives lemma)
//!
//! Order-independent: terms / surfaces sorted lexicographically
//! before hashing.

use std::collections::BTreeMap;

use ox_ontology::concept::ConceptId;
use ox_ontology::glossary::TermLifecycle;
use ox_ontology::OntologyIR;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stable digest of the tokenizer-relevant glossary state.
/// Carried on `ontology_version_snapshots` and diffed at
/// commit_version time to short-circuit rebuilds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryFingerprint(pub String);

impl GlossaryFingerprint {
    pub fn empty() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Compute the fingerprint over the IR's glossary + concepts.
///
/// Contract: stable across orderings, ignores documentation /
/// audit fields. A pure function of (term_id, surfaces,
/// term_pos, lifecycle, concept_id, canonical_term_id).
pub fn glossary_tokenizer_fingerprint(ir: &OntologyIR) -> GlossaryFingerprint {
    let mut hasher = Sha256::new();

    // Section 1: terms — sorted by id.
    let mut terms_sorted: Vec<&_> = ir.glossary().iter().collect();
    terms_sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for term in terms_sorted {
        hasher.update(b"T:");
        hasher.update(term.id.as_str().as_bytes());
        hasher.update(b"|");

        // Lifecycle as a stable wire string. Inactive →
        // emission excluded, but we include lifecycle in the
        // hash so a transition Active↔Deprecated triggers
        // rebuild correctly.
        let lifecycle_tag = match term.lifecycle {
            TermLifecycle::Active => "active",
            TermLifecycle::Deprecated { .. } => "deprecated",
            TermLifecycle::Retired { .. } => "retired",
        };
        hasher.update(lifecycle_tag.as_bytes());
        hasher.update(b"|");

        // POS — affects mecab tag emission.
        let pos_tag = match term.term_pos {
            ox_ontology::glossary::TermPos::Auto => "auto",
            ox_ontology::glossary::TermPos::Noun => "noun",
            ox_ontology::glossary::TermPos::ProperNoun => "proper_noun",
            ox_ontology::glossary::TermPos::Verb => "verb",
            ox_ontology::glossary::TermPos::Adjective => "adjective",
            ox_ontology::glossary::TermPos::Foreign => "foreign",
            ox_ontology::glossary::TermPos::Compound => "compound",
        };
        hasher.update(pos_tag.as_bytes());
        hasher.update(b"|");

        // Concept link — drives canonical lemma. None / Some
        // distinguished + concept_id stable.
        match &term.concept_id {
            Some(c) => {
                hasher.update(b"C:");
                hasher.update(c.as_str().as_bytes());
            }
            None => hasher.update(b"C:_"),
        }
        hasher.update(b"|");

        // Surfaces — primary + translations + aliases. Sorted.
        let mut surfaces: Vec<&str> = Vec::new();
        surfaces.push(term.term.as_str());
        for translation in term.term.translations.values() {
            surfaces.push(translation.as_str());
        }
        for alias in &term.aliases {
            surfaces.push(alias.as_str());
            for translation in alias.translations.values() {
                surfaces.push(translation.as_str());
            }
        }
        surfaces.sort();
        surfaces.dedup();
        for surface in surfaces {
            hasher.update(b"S:");
            hasher.update(surface.as_bytes());
            hasher.update(b"|");
        }

        hasher.update(b"\x1e"); // record separator
    }

    // Section 2: concepts — only canonical_term_id matters for
    // lemma resolution. Other concept fields (description,
    // realisation, ...) don't affect tokenization.
    let concepts: BTreeMap<&ConceptId, &str> = ir
        .concepts()
        .iter()
        .map(|c| (&c.id, c.canonical_term_id.as_str()))
        .collect();
    for (concept_id, canonical_term) in concepts {
        hasher.update(b"K:");
        hasher.update(concept_id.as_str().as_bytes());
        hasher.update(b"|");
        hasher.update(canonical_term.as_bytes());
        hasher.update(b"\x1e");
    }

    GlossaryFingerprint(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::concept::{ConceptDef, ConceptGovernance, ConceptId};
    use ox_ontology::glossary::{
        GlossaryTermDef, GlossaryTermId, TermGovernance, TermPos,
    };

    fn term(id: &str, surface: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(surface),
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
            term_pos: TermPos::Auto,
        }
    }

    fn ir_with_terms(terms: Vec<GlossaryTermDef>) -> OntologyIR {
        let mut ir = OntologyIR::new(
            "ont".into(),
            "test".into(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        for t in terms {
            ir.add_glossary_term(t).unwrap();
        }
        ir
    }

    #[test]
    fn empty_glossary_yields_stable_fingerprint() {
        let ir = ir_with_terms(Vec::new());
        let fp1 = glossary_tokenizer_fingerprint(&ir);
        let fp2 = glossary_tokenizer_fingerprint(&ir);
        assert_eq!(fp1.as_str(), fp2.as_str());
        assert!(!fp1.as_str().is_empty()); // hash of empty input is non-empty
    }

    #[test]
    fn fingerprint_changes_on_surface_change() {
        let ir1 = ir_with_terms(vec![term("gt-1", "고객")]);
        let ir2 = ir_with_terms(vec![term("gt-1", "구매자")]);
        let fp1 = glossary_tokenizer_fingerprint(&ir1);
        let fp2 = glossary_tokenizer_fingerprint(&ir2);
        assert_ne!(fp1.as_str(), fp2.as_str());
    }

    #[test]
    fn fingerprint_changes_on_lifecycle_change() {
        let t1 = term("gt-1", "고객");
        let mut t2 = term("gt-1", "고객");
        t2.lifecycle = TermLifecycle::Retired {
            retired_at: chrono::Utc::now(),
        };
        let fp1 = glossary_tokenizer_fingerprint(&ir_with_terms(vec![t1.clone()]));
        let fp2 = glossary_tokenizer_fingerprint(&ir_with_terms(vec![t2.clone()]));
        assert_ne!(fp1.as_str(), fp2.as_str());
        // Same lifecycle reproduces.
        let fp1b = glossary_tokenizer_fingerprint(&ir_with_terms(vec![t1]));
        assert_eq!(fp1.as_str(), fp1b.as_str());
    }

    #[test]
    fn fingerprint_changes_on_pos_change() {
        let mut t = term("gt-1", "고객");
        t.term_pos = TermPos::Auto;
        let ir1 = ir_with_terms(vec![t.clone()]);
        t.term_pos = TermPos::ProperNoun;
        let ir2 = ir_with_terms(vec![t]);
        assert_ne!(
            glossary_tokenizer_fingerprint(&ir1).as_str(),
            glossary_tokenizer_fingerprint(&ir2).as_str(),
        );
    }

    #[test]
    fn fingerprint_invariant_to_metadata_changes() {
        // description, examples, governance, valid_from/to —
        // none should affect fingerprint.
        let mut t = term("gt-1", "고객");
        let ir1 = ir_with_terms(vec![t.clone()]);
        t.description = LocalizedText::new("새로운 정의");
        t.examples = vec![LocalizedText::new("예시 문장")];
        t.governance.created_at = Some(chrono::Utc::now());
        t.valid_from = Some(chrono::Utc::now());
        let ir2 = ir_with_terms(vec![t]);
        assert_eq!(
            glossary_tokenizer_fingerprint(&ir1).as_str(),
            glossary_tokenizer_fingerprint(&ir2).as_str(),
        );
    }

    #[test]
    fn fingerprint_changes_on_concept_link() {
        let mut t = term("gt-1", "고객");
        let ir1 = ir_with_terms(vec![t.clone()]);
        t.concept_id = Some(ConceptId::new("c-customer"));
        let mut ir2 = ir_with_terms(vec![t]);
        ir2.add_concept(ConceptDef {
            id: ConceptId::new("c-customer"),
            canonical_term_id: GlossaryTermId::new("gt-1"),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: ConceptGovernance::default(),
        })
        .unwrap();
        assert_ne!(
            glossary_tokenizer_fingerprint(&ir1).as_str(),
            glossary_tokenizer_fingerprint(&ir2).as_str(),
        );
    }

    #[test]
    fn fingerprint_invariant_to_term_insertion_order() {
        let t_a = term("gt-a", "alpha");
        let t_b = term("gt-b", "beta");
        let ir1 = ir_with_terms(vec![t_a.clone(), t_b.clone()]);
        let ir2 = ir_with_terms(vec![t_b, t_a]);
        assert_eq!(
            glossary_tokenizer_fingerprint(&ir1).as_str(),
            glossary_tokenizer_fingerprint(&ir2).as_str(),
        );
    }
}

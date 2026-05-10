//! Glossary → lindera user-dictionary CSV compiler.
//!
//! The platform commits to **canonicalisation at the tsvector
//! layer** — every surface form (default + translations +
//! aliases) of a glossary term that's linked to a Concept
//! tokenises to the same canonical lemma, derived from the
//! Concept's `canonical_term_id`. This collapses
//! `LTV` ≡ `고객 생애 가치` ≡ `Customer Lifetime Value` at the
//! lexical retrieval layer (ranker #2 of the hybrid RRF).
//!
//! Surface fidelity is preserved by the trigram ranker (ranker
//! #1 over raw text), so operator-typed `LTV` still matches
//! literal docs containing `LTV` — the canonicalisation only
//! affects the morphological-match axis, not the surface-fuzzy
//! axis.
//!
//! ## CSV format (mecab-ko-dic compatible)
//!
//! `surface,left_id,right_id,cost,POS,POS_subcat1,POS_subcat2,
//! POS_subcat3,conjugation_type,conjugation_form,lemma,
//! reading,pronunciation,...`
//!
//! Lindera's [`UserDictionary::load_from_csv`] consumes this
//! shape directly. We emit:
//! - `surface` = trimmed term surface
//! - `left_id` / `right_id` = `1781` / `3559` (mecab-ko-dic
//!   default IDs for noun-class entries — lindera examples)
//! - `cost` = `-1000` (preferred over system-dict candidates)
//! - `POS` = mecab tag from [`TermPos`]
//! - `lemma` = Concept-canonical surface (or term's own
//!   default surface when no Concept link)
//!
//! ## Lifecycle filter
//!
//! Only `TermLifecycle::Active` terms emit. Deprecated /
//! Retired terms are excluded — they should not influence
//! current retrieval. Their surfaces re-appear as Active
//! `replaced_by` term surfaces (via the alias chain at
//! authoring time) when the operator wants the synonym to
//! survive.

use std::collections::BTreeMap;

use ox_ontology::OntologyIR;
use ox_ontology::concept::{ConceptDef, ConceptId};
use ox_ontology::glossary::{GlossaryTermDef, GlossaryTermId, TermLifecycle, TermPos};
use thiserror::Error;

use crate::tokenizer::TermPosTag;

const DEFAULT_LEFT_ID: u16 = 1781;
const DEFAULT_RIGHT_ID: u16 = 3559;
const DEFAULT_COST: i32 = -1000;
const MIN_SURFACE_LEN: usize = 1;

#[derive(Debug, Error)]
pub enum UserDictCompileError {
    #[error("user dict CSV row write failed: {0}")]
    Write(#[from] std::fmt::Error),
}

/// Compile the workspace's glossary into a lindera user-dict
/// CSV. Returns the CSV body — empty string when the workspace
/// has no Active terms (in which case the registry skips
/// user-dict construction and the tokenizer runs system-only).
///
/// Determinism: surfaces are emitted in (term_id, surface)
/// sorted order so two compiles over an unchanged glossary
/// produce byte-identical CSV — important for fingerprinting
/// + debugging.
pub fn compile_glossary_to_user_dict(ir: &OntologyIR) -> Result<String, UserDictCompileError> {
    // Phase 1: index Concepts by id for canonical-lemma lookup.
    let concepts: BTreeMap<&ConceptId, &ConceptDef> =
        ir.concepts().iter().map(|c| (&c.id, c)).collect();

    // Phase 2: index Active glossary terms by id (canonical
    // surface lookup target). Inactive terms excluded.
    let active_terms: BTreeMap<&GlossaryTermId, &GlossaryTermDef> = ir
        .glossary()
        .iter()
        .filter(|t| matches!(t.lifecycle, TermLifecycle::Active))
        .map(|t| (&t.id, t))
        .collect();

    // Phase 3: emit one CSV row per (term, surface) pair.
    // Surfaces dedupe within a workspace's dict — lindera
    // resolves duplicates by lowest-cost-wins, but emitting
    // duplicates wastes bytes.
    let mut emitted_surfaces: BTreeMap<String, String> = BTreeMap::new();

    for term in active_terms.values() {
        let canonical_lemma = canonical_lemma_for_term(term, &concepts, &active_terms);
        let pos_tag = resolve_pos_tag(term);

        for surface in collect_surfaces(term) {
            let normalised = normalise_surface(&surface);
            if normalised.chars().count() < MIN_SURFACE_LEN {
                continue;
            }
            // First-wins dedup. Sorted iteration ensures
            // deterministic winner across compiles. The lemma
            // for two distinct terms with the same surface is
            // ambiguous — we pick the first by canonical
            // (term_id, surface) ordering and let the operator
            // resolve the conflict via the IR validator (a
            // future invariant).
            emitted_surfaces
                .entry(normalised)
                .or_insert_with(|| build_csv_row(&surface, pos_tag, &canonical_lemma));
        }
    }

    let mut buf = String::with_capacity(emitted_surfaces.len() * 96);
    for (_surface, row) in emitted_surfaces {
        buf.push_str(&row);
        buf.push('\n');
    }
    Ok(buf)
}

/// Resolve the canonical lemma for a term:
/// - Term has `concept_id` + concept exists → look up the
///   concept's canonical glossary term → emit its
///   `term.default` surface (canonicalised) as the lemma. All
///   aliases of the same concept thus share the same lemma —
///   the synonym-collapse mechanic.
/// - Otherwise → term's own `term.default` surface.
fn canonical_lemma_for_term(
    term: &GlossaryTermDef,
    concepts: &BTreeMap<&ConceptId, &ConceptDef>,
    active_terms: &BTreeMap<&GlossaryTermId, &GlossaryTermDef>,
) -> String {
    if let Some(concept_id) = &term.concept_id
        && let Some(concept) = concepts.get(concept_id)
        && let Some(canonical_term) = active_terms.get(&concept.canonical_term_id)
    {
        return canonical_lemma_form(canonical_term.term.as_str());
    }
    canonical_lemma_form(term.term.as_str())
}

/// Canonical lemma encoding for tsvector. Replaces interior
/// whitespace with `_` so a multi-word lemma reads as a single
/// tsvector lexeme. NFC + ASCII-lowercase normalisation.
fn canonical_lemma_form(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.trim().chars() {
        if ch.is_whitespace() {
            out.push('_');
        } else if ch.is_ascii_uppercase() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// All surfaces a single term contributes — primary `term`
/// (default + every translation locale) + every `alias`
/// (default + translations).
fn collect_surfaces(term: &GlossaryTermDef) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.push(term.term.as_str().to_string());
    for translation in term.term.translations.values() {
        out.push(translation.clone());
    }
    for alias in &term.aliases {
        out.push(alias.as_str().to_string());
        for translation in alias.translations.values() {
            out.push(translation.clone());
        }
    }
    out
}

/// `Auto` resolves to a script-based heuristic. Explicit
/// settings pass through unchanged.
fn resolve_pos_tag(term: &GlossaryTermDef) -> TermPosTag {
    match term.term_pos {
        TermPos::Auto => TermPosTag::auto_from_surface(term.term.as_str()),
        TermPos::Noun => TermPosTag::Noun,
        TermPos::ProperNoun => TermPosTag::ProperNoun,
        TermPos::Verb => TermPosTag::Verb,
        TermPos::Adjective => TermPosTag::Adjective,
        TermPos::Foreign => TermPosTag::Foreign,
        TermPos::Compound => TermPosTag::Compound,
    }
}

fn normalise_surface(input: &str) -> String {
    input.trim().to_string()
}

fn build_csv_row(surface: &str, pos: TermPosTag, lemma: &str) -> String {
    format!(
        "{surface},{left},{right},{cost},{pos},*,*,*,*,*,{lemma},*,*",
        surface = csv_escape(surface),
        left = DEFAULT_LEFT_ID,
        right = DEFAULT_RIGHT_ID,
        cost = DEFAULT_COST,
        pos = pos.as_mecab_tag(),
        lemma = csv_escape(lemma),
    )
}

/// CSV field escape. Lindera's CSV parser follows RFC 4180:
/// fields containing `,` / `"` / newline must be quoted, and
/// internal `"` doubled. Our surfaces / lemmas should rarely
/// hit these but the platform supports operator-authored
/// arbitrary surfaces, so be defensive.
fn csv_escape(input: &str) -> String {
    let needs_quoting =
        input.contains(',') || input.contains('"') || input.contains('\n') || input.contains('\r');
    if !needs_quoting {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 4);
    out.push('"');
    for ch in input.chars() {
        if ch == '"' {
            out.push_str("\"\"");
        } else {
            out.push(ch);
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::concept::{ConceptDef, ConceptGovernance, ConceptId};
    use ox_ontology::glossary::{GlossaryTermDef, GlossaryTermId, TermGovernance};

    fn term(
        id: &str,
        surface: &str,
        translations: &[(&str, &str)],
        aliases: &[(&str, &[(&str, &str)])],
        concept_id: Option<&str>,
        pos: TermPos,
    ) -> GlossaryTermDef {
        let mut term_text = LocalizedText::new(surface);
        for (locale, value) in translations {
            term_text
                .translations
                .insert(locale.parse().expect("test locale tag"), value.to_string());
        }
        let aliases: Vec<LocalizedText> = aliases
            .iter()
            .map(|(default, translations)| {
                let mut t = LocalizedText::new(*default);
                for (locale, value) in *translations {
                    t.translations
                        .insert(locale.parse().expect("test locale tag"), value.to_string());
                }
                t
            })
            .collect();
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: term_text,
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases,
            related_terms: Vec::new(),
            governance: TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::Active,
            concept_id: concept_id.map(ConceptId::new),
            term_pos: pos,
        }
    }

    fn ir_with(terms: Vec<GlossaryTermDef>, concepts: Vec<ConceptDef>) -> OntologyIR {
        let mut ir = OntologyIR::new(
            "ont".into(),
            "test".into(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        // Terms must land first — concepts reference
        // GlossaryTermId via canonical_term_id, and the IR
        // validator enforces referential integrity at insert.
        for t in terms {
            ir.add_glossary_term(t).expect("add term");
        }
        for c in concepts {
            ir.add_concept(c).expect("add concept");
        }
        ir
    }

    fn concept(id: &str, canonical_term_id: &str) -> ConceptDef {
        ConceptDef {
            id: ConceptId::new(id),
            canonical_term_id: GlossaryTermId::new(canonical_term_id),
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
        }
    }

    #[test]
    fn empty_glossary_yields_empty_csv() {
        let ir = ir_with(Vec::new(), Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        assert_eq!(csv, "");
    }

    #[test]
    fn surfaces_canonicalise_to_concept_canonical_term() {
        // 두 term 이 같은 concept 을 lexicalize → 둘 다 같은
        // canonical lemma 으로 emit.
        let canonical = term(
            "gt-clv-canonical",
            "고객 생애 가치",
            &[],
            &[],
            Some("c-clv"),
            TermPos::Compound,
        );
        let alias_term = term(
            "gt-clv-ltv",
            "LTV",
            &[],
            &[],
            Some("c-clv"),
            TermPos::Foreign,
        );
        let ir = ir_with(
            vec![canonical, alias_term],
            vec![concept("c-clv", "gt-clv-canonical")],
        );
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        // 두 surface 가 emit 되어야 하지만 lemma 는 같은
        // canonical (`고객_생애_가치`) 로 collapse.
        assert!(csv.contains("고객 생애 가치"), "csv:\n{csv}");
        assert!(csv.contains("LTV"), "csv:\n{csv}");
        // 두 row 모두 lemma=고객_생애_가치
        let lemma_count = csv.matches("고객_생애_가치").count();
        assert!(
            lemma_count >= 2,
            "expected ≥2 occurrences of canonical lemma, got {lemma_count}\n{csv}"
        );
    }

    #[test]
    fn term_without_concept_uses_own_surface_as_lemma() {
        let standalone = term("gt-x", "AS-IS", &[], &[], None, TermPos::Compound);
        let ir = ir_with(vec![standalone], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        // Surface 와 lemma 가 둘 다 AS-IS (lemma 는 normalised "as-is")
        assert!(csv.contains("AS-IS"));
        assert!(csv.contains("as-is"));
    }

    #[test]
    fn deprecated_terms_excluded_from_dict() {
        let mut active = term("gt-active", "고객", &[], &[], None, TermPos::Auto);
        let mut retired = term("gt-old", "구매자", &[], &[], None, TermPos::Auto);
        retired.lifecycle = TermLifecycle::Retired {
            retired_at: chrono::Utc::now(),
        };
        active.lifecycle = TermLifecycle::Active;
        let ir = ir_with(vec![active, retired], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        assert!(csv.contains("고객"));
        assert!(!csv.contains("구매자"), "retired term leaked: {csv}");
    }

    #[test]
    fn translation_surfaces_emit_as_separate_rows() {
        let mut t = term(
            "gt-customer",
            "고객",
            &[("en", "Customer")],
            &[],
            None,
            TermPos::Auto,
        );
        t.lifecycle = TermLifecycle::Active;
        let ir = ir_with(vec![t], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        assert!(csv.contains("고객"));
        assert!(csv.contains("Customer"));
    }

    #[test]
    fn alias_surfaces_emit_with_canonical_lemma() {
        let t = term(
            "gt-customer",
            "고객",
            &[],
            &[("Customer", &[("ko", "구매자")])],
            None,
            TermPos::Auto,
        );
        let ir = ir_with(vec![t], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        // primary + alias default + alias translation
        assert!(csv.contains("고객"));
        assert!(csv.contains("Customer"));
        assert!(csv.contains("구매자"));
    }

    #[test]
    fn dedup_first_wins_on_duplicate_surface() {
        // 두 term 이 같은 surface 를 emit → first-by-id wins
        let t1 = term("gt-1", "공유", &[], &[], None, TermPos::Auto);
        let t2 = term("gt-2", "공유", &[], &[], None, TermPos::Auto);
        let ir = ir_with(vec![t1, t2], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        // Single row only
        let line_count = csv.lines().count();
        assert_eq!(line_count, 1, "duplicate surface emitted twice: {csv}");
    }

    #[test]
    fn empty_surface_skipped() {
        let mut t = term("gt-x", "  ", &[], &[], None, TermPos::Auto);
        t.lifecycle = TermLifecycle::Active;
        let ir = ir_with(vec![t], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        assert_eq!(csv, "");
    }

    #[test]
    fn csv_escapes_special_characters() {
        let t = term(
            "gt-comma",
            "A, B", // contains comma
            &[],
            &[],
            None,
            TermPos::Auto,
        );
        let ir = ir_with(vec![t], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        assert!(
            csv.contains("\"A, B\""),
            "comma surface must be quoted: {csv}"
        );
    }

    #[test]
    fn auto_pos_resolves_per_surface_script() {
        let korean = term("gt-ko", "고객", &[], &[], None, TermPos::Auto);
        let foreign = term("gt-en", "OAuth2", &[], &[], None, TermPos::Auto);
        let mixed = term("gt-mix", "OAuth2 인증", &[], &[], None, TermPos::Auto);
        let ir = ir_with(vec![korean, foreign, mixed], Vec::new());
        let csv = compile_glossary_to_user_dict(&ir).unwrap();
        // 3 lines + tags
        assert_eq!(csv.lines().count(), 3);
        // OAuth2 는 SL (foreign), 고객 / "OAuth2 인증" 은 NNG (compound).
        let oauth_line = csv.lines().find(|l| l.starts_with("OAuth2,")).unwrap();
        assert!(
            oauth_line.contains(",SL,"),
            "OAuth2 should be SL: {oauth_line}"
        );
        let korean_line = csv.lines().find(|l| l.starts_with("고객,")).unwrap();
        assert!(
            korean_line.contains(",NNG,"),
            "고객 should be NNG: {korean_line}"
        );
    }

    #[test]
    fn output_is_deterministic_across_compiles() {
        let t1 = term("gt-1", "A", &[], &[], None, TermPos::Auto);
        let t2 = term("gt-2", "B", &[], &[], None, TermPos::Auto);
        let ir = ir_with(vec![t1, t2], Vec::new());
        let csv1 = compile_glossary_to_user_dict(&ir).unwrap();
        let csv2 = compile_glossary_to_user_dict(&ir).unwrap();
        assert_eq!(csv1, csv2);
    }
}

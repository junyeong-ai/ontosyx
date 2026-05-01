//! Glossary ↔ property binding suggestions — **fall-back layer**
//! after the LLM-side context injection (see
//! [`ox_brain::DesignOntologyInput`]).
//!
//! Φ3 redesign: the design-time prompt now includes the workspace's
//! glossary in `DesignOntologyInput.glossary_terms`, so the LLM sees
//! every canonical term up-front and is expected to bind matching
//! properties at generation time. This module's role narrows to:
//!
//! 1. **Catch what the LLM missed.** Some property ↔ term pairs are
//!    only resolvable by structural / lexical similarity that LLMs
//!    don't always weigh consistently. The scorer flags those
//!    candidates so the admin UI can offer a one-click rebind.
//! 2. **Serve admin-side ad-hoc queries.** "Which properties match
//!    this term?" / "Which terms match this property?" as a
//!    deterministic, explainable surface — no LLM round-trip
//!    required.
//!
//! Operators trust what they can explain, so this stays a *pure
//! function* over `OntologyIR` + `GlossaryTermDef` even if a future
//! variant layers an embedding re-ranker on top. The baseline must
//! still stand alone.
//!
//! Design choices — all biased toward *deterministic, explainable*
//! suggestions:
//!
//! - **Pure function over `OntologyIR` + `GlossaryTermDef`.** No
//!   LLM, no vector store, no network — so tests are fast and the
//!   admin UI can preview changes without an async round-trip. A
//!   future variant may layer an embedding-based re-ranker on top,
//!   but the baseline needs to stand alone.
//!
//! - **Score is a weighted sum of named signals.** Exact match,
//!   alias overlap, description-term overlap, token-prefix fuzzy
//!   match. Each signal contributes a named reason the UI can
//!   render ("matched alias 'customer_tier'", "description shares
//!   3 / 5 terms"). Operators trust what they can explain.
//!
//! - **Bidirectional.** One public function answers "which
//!   properties match this term?"; the inverse ("which terms match
//!   this property?") reuses the same scorer so the two views stay
//!   in sync.
//!
//! The module emits *candidates*; the caller decides whether to
//! auto-apply, batch-confirm, or discard. `OntologyIR` itself is
//! never mutated here.

use std::collections::HashSet;

use ox_core::i18n::LocalizedText;

use crate::glossary::GlossaryTermDef;
use crate::ir::{EdgeTypeDef, EdgeTypeId, NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef,
    PropertyId};

/// Context telling a candidate which entity it belongs to. Surfaced
/// verbatim to the UI so operators can see "this property belongs to
/// `Customer`" without a second lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyOwnerRef {
    Node { node_type: NodeTypeId, label: String },
    Edge { edge_type: EdgeTypeId, label: String },
}

/// One signal that contributed to a candidate's score. Kept as a
/// named enum (not a bare weight) so the UI can group and filter on
/// provenance — e.g. suppress alias-only matches behind a toggle.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingSignal {
    /// Canonical names are identical (case-insensitive).
    CanonicalNameMatch,
    /// The term's canonical name appears in the property's aliases,
    /// or vice versa.
    AliasMatch { matched: String },
    /// Number of term tokens present in the property's description.
    DescriptionOverlap { shared_tokens: u32, total_tokens: u32 },
    /// Levenshtein-ratio-like token-prefix match between canonical
    /// names, used when neither exact nor alias match fires.
    FuzzyNameMatch { ratio_millis: u32 },
}

/// One property-level suggestion. The score is in `[0.0, 1.0]`; the
/// UI typically sorts by score and surfaces the top N.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyBindingCandidate {
    pub owner: PropertyOwnerRef,
    pub property_id: PropertyId,
    pub property_name: String,
    pub score: f32,
    pub signals: Vec<BindingSignal>,
}

/// Knobs for the scorer. The defaults are picked so a sample
/// ontology (Customer / Order / Product / Brand) produces no
/// false positives against an unrelated glossary term ("weather")
/// while picking up the obvious candidates for a matching term
/// ("VIP grade").
#[derive(Debug, Clone, Copy)]
pub struct BindingSuggestionPolicy {
    pub min_score: f32,
    pub max_results: usize,
    pub weight_exact_name: f32,
    pub weight_alias_match: f32,
    pub weight_description_overlap: f32,
    pub weight_fuzzy_name: f32,
    pub fuzzy_min_ratio: f32,
    /// When `true`, properties that already carry a `glossary_term_id`
    /// are skipped entirely. Most callers want this — the point of
    /// the suggestion flow is to bind *unbound* properties.
    pub skip_already_bound: bool,
}

impl Default for BindingSuggestionPolicy {
    fn default() -> Self {
        Self {
            min_score: 0.3,
            max_results: 20,
            weight_exact_name: 1.0,
            weight_alias_match: 0.8,
            weight_description_overlap: 0.5,
            weight_fuzzy_name: 0.4,
            fuzzy_min_ratio: 0.7,
            skip_already_bound: true,
        }
    }
}

/// Score every unbound property in the ontology against `term` and
/// return those whose combined score clears `policy.min_score`,
/// sorted by descending score.
pub fn suggest_property_bindings_by_term(
    ontology: &OntologyIR,
    term: &GlossaryTermDef,
    policy: BindingSuggestionPolicy,
) -> Vec<PropertyBindingCandidate> {
    let term_signals = TermSignals::from_term(term);
    let mut out: Vec<PropertyBindingCandidate> = Vec::new();

    for node in ontology.node_types() {
        let owner = || PropertyOwnerRef::Node {
            node_type: node.id.clone(),
            label: node.label.to_string(),
        };
        collect_candidates(&term_signals, node.properties.iter(), owner, policy, &mut out);
    }
    for edge in ontology.edge_types() {
        let owner = || PropertyOwnerRef::Edge {
            edge_type: edge.id.clone(),
            label: edge.label.to_string(),
        };
        collect_candidates(&term_signals, edge.properties.iter(), owner, policy, &mut out);
    }

    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(policy.max_results);
    out
}

/// Inverse direction: given a property reference, return the
/// glossary terms most likely to describe it. Useful when the
/// operator is editing a property and wants a single-click "link to
/// existing term" suggestion.
pub fn suggest_terms_by_property(
    ontology: &OntologyIR,
    prop_ref: &PropertyOwnerRef,
    property_id: &PropertyId,
    policy: BindingSuggestionPolicy,
) -> Vec<TermBindingCandidate> {
    let Some(prop) = locate_property(ontology, prop_ref, property_id) else {
        return Vec::new();
    };
    let property_signals = PropertySignals::from_property(prop);
    let mut out: Vec<TermBindingCandidate> = Vec::new();
    for term in ontology.glossary() {
        let (score, signals) = score_term_against_property(term, &property_signals, policy);
        if score >= policy.min_score {
            out.push(TermBindingCandidate {
                term_id: term.id.clone(),
                term: term.term.clone(),
                score,
                signals,
            });
        }
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(policy.max_results);
    out
}

/// Mirror of `PropertyBindingCandidate` for the inverse direction.
#[derive(Debug, Clone, PartialEq)]
pub struct TermBindingCandidate {
    pub term_id: crate::glossary::GlossaryTermId,
    pub term: ox_core::i18n::LocalizedText,
    pub score: f32,
    pub signals: Vec<BindingSignal>,
}

// ---------------------------------------------------------------------------
// Implementation — tokenisation + scoring
// ---------------------------------------------------------------------------

struct TermSignals<'a> {
    canonical: String,
    aliases: Vec<String>,
    description_tokens: HashSet<String>,
    raw_term: &'a GlossaryTermDef,
}

impl<'a> TermSignals<'a> {
    fn from_term(term: &'a GlossaryTermDef) -> Self {
        // Canonical key uses the term's `default` locale: stable across
        // translation churn so a property scored against the term in
        // ko vs en gets the same canonical anchor. Per-locale variants
        // of `term` and `display_name` enrich the alias surface.
        let canonical = normalise(&term.term.default);
        let mut aliases: Vec<String> = term
            .aliases
            .iter()
            .flat_map(|alias| localized_values(alias).map(|v| normalise(&v)))
            .collect();
        aliases.extend(localized_values(&term.term).map(|v| normalise(&v)));
        aliases.extend(localized_values(&term.display_name).map(|v| normalise(&v)));
        aliases.retain(|a: &String| !a.is_empty() && *a != canonical);
        aliases.sort();
        aliases.dedup();
        let description_tokens = localized_values(&term.description)
            .flat_map(|v| tokenise(&v).into_iter())
            .collect::<HashSet<_>>();
        Self {
            canonical,
            aliases,
            description_tokens,
            raw_term: term,
        }
    }
}

struct PropertySignals {
    canonical: String,
    aliases: Vec<String>,
    description_tokens: HashSet<String>,
    business_context_tokens: HashSet<String>,
}

impl PropertySignals {
    fn from_property(prop: &PropertyDef) -> Self {
        let canonical = normalise(prop.name.as_str());
        let mut aliases: Vec<String> = prop
            .aliases
            .iter()
            .flat_map(|text| localized_values(text).map(|v| normalise(&v)))
            .collect();
        aliases.extend(localized_values(&prop.display_name).map(|v| normalise(&v)));
        aliases.retain(|a: &String| !a.is_empty() && *a != canonical);
        aliases.sort();
        aliases.dedup();
        let description_tokens = localized_values(&prop.description)
            .flat_map(|v| tokenise(&v).into_iter())
            .collect();
        let business_context_tokens = localized_values(&prop.business_context)
            .flat_map(|v| tokenise(&v).into_iter())
            .collect();
        Self {
            canonical,
            aliases,
            description_tokens,
            business_context_tokens,
        }
    }
}

fn collect_candidates<'a, F, I>(
    term: &TermSignals<'_>,
    properties: I,
    mut owner: F,
    policy: BindingSuggestionPolicy,
    out: &mut Vec<PropertyBindingCandidate>,
) where
    F: FnMut() -> PropertyOwnerRef,
    I: IntoIterator<Item = &'a PropertyDef>,
{
    for prop in properties {
        if policy.skip_already_bound && prop.glossary_term_id().is_some() {
            continue;
        }
        let property = PropertySignals::from_property(prop);
        let (score, signals) = score_term_against_property_raw(term, &property, policy);
        if score >= policy.min_score {
            out.push(PropertyBindingCandidate {
                owner: owner(),
                property_id: prop.id.clone(),
                property_name: prop.name.to_string(),
                score,
                signals,
            });
        }
    }
}

fn score_term_against_property(
    term: &GlossaryTermDef,
    property: &PropertySignals,
    policy: BindingSuggestionPolicy,
) -> (f32, Vec<BindingSignal>) {
    let term_signals = TermSignals::from_term(term);
    score_term_against_property_raw(&term_signals, property, policy)
}

fn score_term_against_property_raw(
    term: &TermSignals<'_>,
    property: &PropertySignals,
    policy: BindingSuggestionPolicy,
) -> (f32, Vec<BindingSignal>) {
    let mut signals = Vec::new();
    let mut score: f32 = 0.0;

    if !term.canonical.is_empty() && term.canonical == property.canonical {
        score += policy.weight_exact_name;
        signals.push(BindingSignal::CanonicalNameMatch);
    } else if let Some(matched) = alias_match(term, property) {
        score += policy.weight_alias_match;
        signals.push(BindingSignal::AliasMatch { matched });
    } else {
        let ratio = fuzzy_ratio(&term.canonical, &property.canonical);
        if ratio >= policy.fuzzy_min_ratio {
            score += policy.weight_fuzzy_name * ratio;
            signals.push(BindingSignal::FuzzyNameMatch {
                ratio_millis: (ratio * 1000.0) as u32,
            });
        }
    }

    // Description overlap counts tokens from the *term*'s description
    // present in the property's description **or** business-context.
    // Business context is the operator-supplied free-form note and
    // often holds the clearest match when names diverge.
    let total_tokens = term.description_tokens.len() as u32;
    if total_tokens > 0 {
        let combined: HashSet<&String> = property
            .description_tokens
            .iter()
            .chain(property.business_context_tokens.iter())
            .collect();
        let shared = term
            .description_tokens
            .iter()
            .filter(|t| combined.contains(*t))
            .count() as u32;
        if shared > 0 {
            let ratio = shared as f32 / total_tokens as f32;
            score += policy.weight_description_overlap * ratio;
            signals.push(BindingSignal::DescriptionOverlap {
                shared_tokens: shared,
                total_tokens,
            });
        }
    }

    // Clamp score so the downstream UI sees a well-bounded value.
    let clamped = score.clamp(0.0, 1.0);
    // Hack-free "no evidence" escape: if no signals fired, the score
    // is zero regardless of the weighted sum rounding.
    if signals.is_empty() {
        return (0.0, signals);
    }
    (clamped, signals)
}

fn alias_match(term: &TermSignals<'_>, property: &PropertySignals) -> Option<String> {
    // Term alias in property identifiers?
    for alias in &term.aliases {
        if alias == &property.canonical {
            return Some(alias.clone());
        }
        if property.aliases.iter().any(|p| p == alias) {
            return Some(alias.clone());
        }
    }
    // Term canonical in property aliases?
    for alias in &property.aliases {
        if alias == &term.canonical {
            return Some(alias.clone());
        }
        if term.aliases.iter().any(|t| t == alias) {
            return Some(alias.clone());
        }
    }
    // Also treat matches_text() equivalence as a hit — cheap safety
    // net for locale / casing quirks the normalise step missed.
    if term.raw_term.matches_text(&property.canonical) {
        return Some(property.canonical.clone());
    }
    None
}

fn locate_property<'a>(
    ontology: &'a OntologyIR,
    owner: &PropertyOwnerRef,
    property_id: &PropertyId,
) -> Option<&'a PropertyDef> {
    match owner {
        PropertyOwnerRef::Node { node_type, .. } => ontology
            .node_types()
            .iter()
            .find(|n: &&NodeTypeDef| n.id == *node_type)
            .and_then(|n| n.properties.iter().find(|p| p.id == *property_id)),
        PropertyOwnerRef::Edge { edge_type, .. } => ontology
            .edge_types()
            .iter()
            .find(|e: &&EdgeTypeDef| e.id == *edge_type)
            .and_then(|e| e.properties.iter().find(|p| p.id == *property_id)),
    }
}

fn normalise(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn tokenise(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| s.len() > 1) // drop single-character noise
        .collect()
}

/// Iterate over every non-empty value in a `LocalizedText`. The
/// `LocalizedText::iter()` API yields `(None, default)` then
/// `(Some(tag), translation)` for every locale; we want **all**
/// non-empty entries so a Korean display + English fallback both
/// participate in scoring.
fn localized_values(text: &LocalizedText) -> impl Iterator<Item = String> + '_ {
    text.iter().filter_map(|(_, v)| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Bigram / prefix-based fuzzy ratio in `[0.0, 1.0]`. Full
/// Levenshtein is overkill for short property names; a bigram
/// Dice coefficient picks up common typos (`customr` vs
/// `customer`) and prefix matches (`cust` ↔ `customer`) without
/// dragging in the `strsim` crate.
fn fuzzy_ratio(a: &str, b: &str) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let bigrams_a = bigrams(a);
    let bigrams_b = bigrams(b);
    if bigrams_a.is_empty() || bigrams_b.is_empty() {
        return 0.0;
    }
    let common = bigrams_a.iter().filter(|bg| bigrams_b.contains(bg)).count();
    (2.0 * common as f32) / (bigrams_a.len() + bigrams_b.len()) as f32
}

fn bigrams(value: &str) -> Vec<[char; 2]> {
    let chars: Vec<char> = value.chars().collect();
    chars.windows(2).map(|w| [w[0], w[1]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_system::CodeSystemDef;
    use crate::glossary::{GlossaryTermDef, GlossaryTermId};
    use crate::ir::{Cardinality, NodeTypeDef, OntologyIR, PropertyDef, PropertyId};
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    fn property(name: &str, description: &str) -> PropertyDef {
        PropertyDef {
            id: PropertyId::new(format!("p-{name}")),
            name: PropertyKey::new(name).expect("valid property name"),
            display_name: LocalizedText::default(),
            property_type: PropertyType::String,
            nullable: true,
            default_value: None,
            description: LocalizedText::new(description),
            classification: None,
            ..Default::default()
        }
    }

    fn bound_property(name: &str, description: &str, bound_to: &str) -> PropertyDef {
        let mut p = property(name, description);
        p.bindings.push(crate::binding::PropertyBinding::glossary(GlossaryTermId::new(bound_to),));
        p
    }

    fn node(label: &str, props: Vec<PropertyDef>) -> NodeTypeDef {
        NodeTypeDef {
            id: format!("n-{label}").into(),
            label: GraphLabel::new(label).expect("valid label"),
            description: LocalizedText::default(),
            properties: props,
            constraints: vec![],
            ..Default::default()
        }
    }

    fn ontology(nodes: Vec<NodeTypeDef>, glossary: Vec<GlossaryTermDef>) -> OntologyIR {
        let mut ir = OntologyIR::new(
            "ont-test".to_string(),
            "Test".to_string(),
            LocalizedText::default(),
            1,
            nodes,
            Vec::new(),
            Vec::new(),
        );
        for t in glossary {
            ir.add_glossary_term(t).expect("glossary add");
        }
        ir
    }

    fn term(id: &str, name: &str, aliases: &[&str], description: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(name),
            display_name: LocalizedText::default(),
            description: LocalizedText::new(description),
            examples: Vec::new(),
            category: None,
            aliases: aliases.iter().map(|s| LocalizedText::new(*s)).collect(),
            related_terms: Vec::new(),
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
        realisation: None,
        }
    }

    #[test]
    fn canonical_name_match_scores_highest() {
        let ir = ontology(
            vec![node("Customer", vec![property("customer_grade", "")])],
            vec![],
        );
        let t = term("t1", "customer_grade", &[], "");
        let out = suggest_property_bindings_by_term(&ir, &t, Default::default());
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].signals[0], BindingSignal::CanonicalNameMatch));
        assert!(out[0].score >= 0.99);
    }

    #[test]
    fn alias_match_fires_when_canonical_differs() {
        let ir = ontology(
            vec![node("Customer", vec![property("vip_tier", "")])],
            vec![],
        );
        let t = term("t1", "VIP Grade", &["vip_tier"], "");
        let out = suggest_property_bindings_by_term(&ir, &t, Default::default());
        assert_eq!(out.len(), 1);
        assert!(out[0]
            .signals
            .iter()
            .any(|s| matches!(s, BindingSignal::AliasMatch { .. })));
    }

    #[test]
    fn description_overlap_contributes_when_names_diverge() {
        let ir = ontology(
            vec![node(
                "Customer",
                vec![property(
                    "segment_bucket",
                    "Marketing segment rollup for campaign eligibility",
                )],
            )],
            vec![],
        );
        let t = term(
            "t1",
            "marketing_segment",
            &[],
            "Marketing segment used for campaign eligibility",
        );
        let out = suggest_property_bindings_by_term(&ir, &t, Default::default());
        assert!(!out.is_empty(), "expected description overlap to fire");
        assert!(out[0]
            .signals
            .iter()
            .any(|s| matches!(s, BindingSignal::DescriptionOverlap { .. })));
    }

    #[test]
    fn unrelated_term_yields_no_candidates() {
        let ir = ontology(
            vec![node("Customer", vec![property("customer_grade", "")])],
            vec![],
        );
        let t = term("t1", "weather", &[], "atmospheric conditions");
        let out = suggest_property_bindings_by_term(&ir, &t, Default::default());
        assert!(out.is_empty());
    }

    #[test]
    fn already_bound_property_is_skipped() {
        let p = bound_property("customer_grade", "", "existing");
        let ir = ontology(vec![node("Customer", vec![p])], vec![]);
        let t = term("t1", "customer_grade", &[], "");
        let out = suggest_property_bindings_by_term(&ir, &t, Default::default());
        assert!(out.is_empty());
    }

    #[test]
    fn fuzzy_name_match_picks_up_typos() {
        let ir = ontology(
            vec![node("Customer", vec![property("customer_grade", "")])],
            vec![],
        );
        let t = term("t1", "customr_grade", &[], "");
        let policy = BindingSuggestionPolicy {
            weight_fuzzy_name: 1.0, // lift fuzzy so it clears min_score alone
            ..Default::default()
        };
        let out = suggest_property_bindings_by_term(&ir, &t, policy);
        assert!(!out.is_empty());
        assert!(out[0]
            .signals
            .iter()
            .any(|s| matches!(s, BindingSignal::FuzzyNameMatch { .. })));
    }

    #[test]
    fn bidirectional_suggest_terms_returns_same_term_for_matching_property() {
        let t = term("t1", "customer_grade", &[], "");
        let ir = ontology(
            vec![node("Customer", vec![property("customer_grade", "")])],
            vec![t.clone()],
        );
        // Pick the property we seeded.
        let node_ref = PropertyOwnerRef::Node {
            node_type: ir.node_types()[0].id.clone(),
            label: ir.node_types()[0].label.to_string(),
        };
        let pid = ir.node_types()[0].properties[0].id.clone();
        let out = suggest_terms_by_property(&ir, &node_ref, &pid, Default::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term.default, "customer_grade");
    }

    #[test]
    fn max_results_truncates() {
        let props: Vec<PropertyDef> = (0..30)
            .map(|i| property(&format!("customer_grade_{i}"), ""))
            .collect();
        let ir = ontology(vec![node("Customer", props)], vec![]);
        let t = term("t1", "customer_grade", &[], "");
        let policy = BindingSuggestionPolicy {
            max_results: 5,
            ..Default::default()
        };
        let out = suggest_property_bindings_by_term(&ir, &t, policy);
        assert!(out.len() <= 5);
    }

    // Required to silence unused import in tests.
    #[allow(dead_code)]
    fn _unused(_: &CodeSystemDef, _: &Cardinality) {}
}

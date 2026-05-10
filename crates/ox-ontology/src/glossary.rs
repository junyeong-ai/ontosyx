//! Business-glossary terms and taxonomies.
//!
//! A glossary lets the platform record domain language separately
//! from the technical ontology shape. Business concepts own the
//! semantic identity; glossary terms provide the preferred labels,
//! aliases, descriptions, examples, and taxonomy edges that make
//! those concepts usable across locales and source-system wording.
//!
//! The data model follows W3C SKOS Core + SKOS-XL + OMG SBVR. Each
//! `GlossaryTermDef` carries:
//!
//! - **Identity**: stable [`GlossaryTermId`].
//! - **Display**: localised [`LocalizedText`] for `term`, `display_name`,
//!   `description`, and per-locale `aliases` so the glossary speaks
//!   the operator's language end-to-end.
//! - **Examples**: localised illustrative sentences that disambiguate
//!   abstract definitions ("Customer: 'A buyer of any sales channel'.
//!   *Example:* a marketplace partner who places one order per year").
//! - **SKOS relations** ([`TermRelation`]): hierarchy, association,
//!   equivalence to other terms.
//! - **Governance** ([`TermGovernance`]): origin, authorship, and
//!   editorial trail (scope notes, editorial notes, change notes —
//!   SKOS-XL `xl:scopeNote` / `xl:editorialNote` / `xl:changeNote`).
//! - **Validity window** (`valid_from`, `valid_to`): the period during
//!   which the term's semantics apply. Independent of binding-level
//!   windows on `PropertyDef` so a term can age out of use without
//!   touching every property that referenced it.
//! - **Lifecycle** ([`TermLifecycle`]): active, deprecated (with a
//!   pointer to its replacement so "use X instead" is a structured
//!   field), or retired (soft-deleted but kept for history).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::function::FunctionId;
use crate::segment::SegmentId;

ox_core::define_id_newtype!(
    /// Stable identifier for a glossary term.
    GlossaryTermId
);

ox_core::define_id_newtype!(
    /// Stable identifier for a taxonomy (a named tree view over a
    /// subset of glossary terms).
    TaxonomyId
);

/// Atomic unit of the glossary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct GlossaryTermDef {
    pub id: GlossaryTermId,

    /// Canonical preferred label. `LocalizedText` so deployments that
    /// surface terms in multiple languages can ship one record per
    /// concept (English `Customer`, Korean `고객`) instead of two
    /// duplicate records linked by ad-hoc convention.
    pub term: LocalizedText,

    /// Operator-facing display label. Most callers want `term`; the
    /// distinction matters when the catalogue UI wants a longer
    /// human-friendly form ("Active Paying Customer") while
    /// downstream code keeps a compact identifier label
    /// ("paying_customer").
    #[serde(default)]
    pub display_name: LocalizedText,

    /// Domain definition. Longer than the term, localised so a
    /// bilingual deployment can ship English + Korean text without
    /// inventing a second glossary store.
    #[serde(default)]
    pub description: LocalizedText,

    /// Illustrative examples that disambiguate abstract definitions.
    /// Each entry is a self-contained sentence. SKOS `skos:example`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<LocalizedText>,

    /// Author-supplied category (e.g. `"business_concept"`,
    /// `"measure"`, `"dimension"`). No fixed taxonomy — categories
    /// are tenant-defined so the glossary doesn't force an upstream
    /// ontology.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Alternate names per locale (synonyms, abbreviations). Used for
    /// synonym-aware search in the glossary UI and for LLM prompts
    /// that need to normalise arbitrary user phrasing onto a term.
    /// SKOS `skos:altLabel`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<LocalizedText>,

    /// SKOS relations to other terms — hierarchy / association /
    /// equivalence — encoded from this term's perspective.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_terms: Vec<TermRelation>,

    /// Authorship and editorial trail.
    #[serde(default)]
    pub governance: TermGovernance,

    /// Inclusive lower bound on the term's validity period. `None`
    /// means "valid since the beginning of the ontology lineage".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,

    /// Exclusive upper bound on the term's validity period. `None`
    /// means open-ended. A retired term keeps its `valid_to` set so
    /// historical queries can still reference the concept as it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,

    /// Lifecycle state. Drives UI affordances (deprecation strikethrough,
    /// retired filter) and downstream resolvers (NL-to-Cypher should
    /// route a deprecated term through its `replaced_by` automatically).
    #[serde(default)]
    pub lifecycle: TermLifecycle,

    /// Concept this term lexicalizes — `Some(concept_id)` declares
    /// the term as a `prefLabel` / `altLabel` of a registered
    /// [`crate::concept::ConceptDef`]. The concept owns the
    /// executable realisation (segment / function / cross-entity
    /// predicate); terms with no `concept_id` are pure lexicon
    /// entries (definitions, alias spellings) that the catalogue
    /// hasn't yet promoted into the concept layer. The IR
    /// validator rejects an anchor that doesn't resolve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept_id: Option<crate::concept::ConceptId>,

    /// POS hint for the morphological tokenizer's user
    /// dictionary. Φ-text substrate compiles every Active term
    /// into a lindera user-dict CSV row keyed by surface; the
    /// POS field controls how lindera segments adjacent text
    /// against the term. Default [`TermPos::Auto`] derives the
    /// tag from the surface's script (Korean → Compound,
    /// English-only → Foreign, mixed → Compound) — operator
    /// overrides per term when the heuristic misclassifies
    /// (e.g. a verb-form term `재인증하다` needs explicit
    /// [`TermPos::Verb`]).
    #[serde(default)]
    pub term_pos: TermPos,
}

/// Closed POS surface the platform emits to lindera user
/// dictionaries. Mirrors the subset of mecab-ko-dic tags the
/// `ox-text` indexable-POS filter retains; emitting a non-keep
/// tag would silently drop the dict entry at query time.
///
/// `Auto` is the default — script-based heuristic lets
/// operators add terms without thinking about POS unless they
/// hit the rare misclassification edge.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TermPos {
    /// Heuristic from surface script (default).
    #[default]
    Auto,
    /// Common noun (NNG).
    Noun,
    /// Proper noun (NNP).
    ProperNoun,
    /// Verb stem (VV) — for terms like `재인증하다` that act
    /// verbally in Korean text.
    Verb,
    /// Adjective stem (VA).
    Adjective,
    /// Foreign word / loanword (SL) — pure ASCII alphanumeric
    /// terms, technical acronyms, brand names.
    Foreign,
    /// Compound noun (NNG with multi-token surface) — mixed
    /// Korean + non-Korean compounds like `OAuth2 인증`.
    Compound,
}

/// Executable realisation of a business concept — how the runtime
/// decides membership / value at query time.
///
/// `Segment` is the canonical case: "Active Customer = Customer whose
/// last_order < 90 days" lowers to a `SegmentDef`. `Function` covers
/// computed-value concepts ("Lifetime Value = sum of order totals").
/// `CrossEntity` is the structured-predicate escape for the rare case
/// neither shape fits — the predicate is parsed by the planner
/// against the concept's implementing NodeTypes.
///
/// `Query` (saved-view realisation) was deliberately rejected:
/// `InsightId` lives in `ox-store`, not the IR, and layering
/// `ox-ontology → ox-store` would break the dependency arrow
/// `ox-core ← ox-ontology ← ox-store`. View-shaped concepts instead
/// use a `Function` whose body returns the saved-view rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TermRealisation {
    Segment {
        segment_id: SegmentId,
    },
    Function {
        function_id: FunctionId,
    },
    /// Free-form predicate evaluated against the term's implementing
    /// NodeTypes. Surfaced to the planner as a structured filter —
    /// the predicate's properties must resolve on at least one
    /// implementer for the term to validate.
    CrossEntity {
        predicate: String,
    },
}

/// One SKOS-style edge between two glossary terms.
///
/// Relations are stored on the term whose perspective they describe
/// (`source` is the term the edge "lives on"). Inverse edges are
/// authored separately when both directions are meaningful — a
/// `Broader → t-parent` lives on the child term, and the parent
/// carries `Narrower → t-child` on its own list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct TermRelation {
    pub kind: TermRelationKind,
    pub target: GlossaryTermId,
}

/// SKOS-aligned relation kinds.
///
/// Reference: <https://www.w3.org/TR/skos-reference/>. The four
/// hierarchy / lateral kinds (`Broader`, `Narrower`, `Related`,
/// `SeeAlso`) plus the two equivalence kinds (`ExactMatch`,
/// `CloseMatch`) cover the canonical SKOS surface for vocabulary
/// alignment.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TermRelationKind {
    /// `target` is a more general concept (parent in a hierarchy).
    Broader,
    /// `target` is a more specific concept (child in a hierarchy).
    Narrower,
    /// `target` is associated but not hierarchical.
    Related,
    /// Cross-reference for the reader; weaker than `Related`.
    SeeAlso,
    /// `target` denotes the same concept (high confidence).
    ExactMatch,
    /// `target` denotes the same concept (lower confidence — useful
    /// for cross-vocabulary alignment imports).
    CloseMatch,
}

/// Authorship and editorial trail. SKOS-XL `dct:creator`,
/// `dct:created`, `xl:scopeNote`, `xl:editorialNote`, `xl:changeNote`.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct TermGovernance {
    /// How the term entered the glossary.
    #[serde(default)]
    pub origin: TermOrigin,

    /// Identifier of the principal who authored the term. `None`
    /// when the term was machine-derived without a human author
    /// (`origin = DerivedFromColumn`, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// When the term was first added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,

    /// Scope notes — clarifications about applicability ("only in
    /// retail context", "excludes returns"). One entry per locale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_notes: Vec<LocalizedText>,

    /// Editorial notes — instructions to glossary maintainers
    /// ("verify against legal vocabulary"). Internal, not for the
    /// catalogue surface.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editorial_notes: Vec<LocalizedText>,

    /// Append-only change log. Each entry records one revision of
    /// the term (definition rewrite, alias added, deprecation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_notes: Vec<TermChangeNote>,
}

/// Where the term originated. Drives UI affordances like a "machine
/// suggestion" badge that prompts a human to validate.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TermOrigin {
    /// Authored by hand in the glossary UI or imported via authored
    /// seed file.
    #[default]
    Manual,
    /// Auto-extracted from a source-data column during analysis. The
    /// FE shows an "AI-suggested" marker; an operator must confirm
    /// before the term participates in NL routing.
    DerivedFromColumn { table: String, column: String },
    /// Imported from an external catalogue (Collibra, Atlan, …). The
    /// catalogue identifier lets a future re-sync match terms back
    /// to their upstream record.
    ImportedFrom {
        catalog: String,
        external_id: Option<String>,
    },
}

/// One entry in a term's append-only change log.
///
/// `provenance` is the optional bridge to the workspace-wide PROV-O
/// graph: when a change was driven by a structured activity (a
/// chat-driven edit, an LLM enrichment pass, an external import),
/// the corresponding [`crate::provenance::ProvenanceDef`] id lives
/// here so consumers can walk back to the agent / activity / source
/// without parsing the prose `note`. Routine manual edits leave it
/// `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct TermChangeNote {
    pub at: DateTime<Utc>,
    /// Principal who made the change. `None` for system-driven
    /// rewrites (auto-enrichment, bulk import).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    pub note: LocalizedText,
    /// Optional pointer to the workspace-wide PROV-O entry that
    /// captured the activity behind this change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<crate::provenance::ProvenanceId>,
}

/// Lifecycle state. New terms are `Active`; deprecation points to a
/// successor; retirement keeps the term in history but excludes it
/// from active resolution.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TermLifecycle {
    /// In active use. Default for new terms.
    #[default]
    Active,
    /// Discouraged but still resolvable. `replaced_by` carries the
    /// successor term so NL-to-Cypher / catalogue UI can offer a
    /// "use X instead" hint without authoring a separate redirect
    /// table.
    Deprecated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaced_by: Option<GlossaryTermId>,
        deprecated_at: DateTime<Utc>,
    },
    /// Soft-deleted. Excluded from active resolvers but retained in
    /// the IR so historical query plans / load lineage referencing
    /// the term continue to compile.
    Retired { retired_at: DateTime<Utc> },
}

impl GlossaryTermDef {
    /// Does the candidate string match this term, an alias, or the
    /// display name in any locale (case-insensitive, trimmed)? Used
    /// by incremental imports to merge two descriptions of the same
    /// concept without duplicating the record.
    pub fn matches_text(&self, candidate: &str) -> bool {
        let needle = candidate.trim().to_lowercase();
        if needle.is_empty() {
            return false;
        }
        if locales_contain_ci(&self.term, &needle) {
            return true;
        }
        if locales_contain_ci(&self.display_name, &needle) {
            return true;
        }
        self.aliases
            .iter()
            .any(|alias| locales_contain_ci(alias, &needle))
    }

    /// True when the term is in [`TermLifecycle::Active`] state.
    /// Convenience for the common filter "all terms a resolver may
    /// route a user query through".
    pub fn is_active(&self) -> bool {
        matches!(self.lifecycle, TermLifecycle::Active)
    }

    /// True when `at` falls inside the term's `[valid_from, valid_to)`
    /// window. Open ends mean unbounded on that side.
    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool {
        if let Some(from) = self.valid_from
            && at < from
        {
            return false;
        }
        if let Some(to) = self.valid_to
            && at >= to
        {
            return false;
        }
        true
    }

    /// Successor term id when this term is deprecated, otherwise
    /// `None`. Used by NL routing to silently substitute the
    /// successor while keeping a "deprecated" badge in the UI.
    pub fn replaced_by(&self) -> Option<&GlossaryTermId> {
        match &self.lifecycle {
            TermLifecycle::Deprecated { replaced_by, .. } => replaced_by.as_ref(),
            _ => None,
        }
    }
}

fn locales_contain_ci(text: &LocalizedText, needle_lower: &str) -> bool {
    if text.default.to_lowercase() == needle_lower {
        return true;
    }
    text.translations
        .values()
        .any(|v| v.to_lowercase() == needle_lower)
}

/// A named tree view over a subset of glossary terms.
///
/// `TaxonomyNode` is intentionally tree-shaped — cross-links are
/// modelled as separate taxonomies rather than a DAG, so a slow
/// traversal through a deeply-nested industry classification always
/// terminates without cycle detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct TaxonomyDef {
    pub id: TaxonomyId,

    /// Short name: `"Industries"`, `"Customer Segments"`.
    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    /// Tree root. A taxonomy with no root is a catalogue — the UI
    /// shows the flat term list; the `root` shape lets a taxonomy
    /// optionally express depth.
    pub root: TaxonomyNode,
}

/// One node in a taxonomy tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct TaxonomyNode {
    pub term_id: GlossaryTermId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TaxonomyNode>,
}

impl TaxonomyNode {
    /// Walk the tree in pre-order, yielding every term id.
    pub fn walk(&self, visit: &mut impl FnMut(&GlossaryTermId)) {
        visit(&self.term_id);
        for child in &self.children {
            child.walk(visit);
        }
    }

    /// Count every descendant (inclusive). O(n).
    pub fn size(&self) -> usize {
        let mut n = 0;
        self.walk(&mut |_| n += 1);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(id: &str, text: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: LocalizedText::new(text),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            aliases: Vec::new(),
            related_terms: Vec::new(),
            governance: TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: TermLifecycle::default(),
            concept_id: None,
            term_pos: Default::default(),
        }
    }

    #[test]
    fn matches_text_is_case_insensitive_and_trims() {
        let mut t = term("t-1", "Customer");
        t.aliases.push(LocalizedText::new("Client"));
        assert!(t.matches_text("customer"));
        assert!(t.matches_text("  CUSTOMER "));
        assert!(t.matches_text("client"));
        assert!(!t.matches_text("vendor"));
    }

    #[test]
    fn matches_text_finds_translated_locale_label() {
        let mut t = term("t-1", "Customer");
        t.term = LocalizedText::new("Customer")
            .with_translation(ox_core::i18n::LanguageTag::ko(), "고객");
        assert!(t.matches_text("고객"));
        assert!(t.matches_text("customer"));
    }

    #[test]
    fn taxonomy_walk_visits_every_term_in_pre_order() {
        let root = TaxonomyNode {
            term_id: GlossaryTermId::new("t-root"),
            children: vec![
                TaxonomyNode {
                    term_id: GlossaryTermId::new("t-a"),
                    children: vec![TaxonomyNode {
                        term_id: GlossaryTermId::new("t-a-1"),
                        children: vec![],
                    }],
                },
                TaxonomyNode {
                    term_id: GlossaryTermId::new("t-b"),
                    children: vec![],
                },
            ],
        };
        let mut seen = Vec::new();
        root.walk(&mut |id| seen.push(id.to_string()));
        assert_eq!(seen, vec!["t-root", "t-a", "t-a-1", "t-b"]);
        assert_eq!(root.size(), 4);
    }

    #[test]
    fn lifecycle_default_is_active() {
        let t = term("t-1", "Customer");
        assert!(t.is_active());
        assert!(t.replaced_by().is_none());
    }

    #[test]
    fn deprecated_term_exposes_replacement() {
        let mut t = term("t-old", "Client");
        t.lifecycle = TermLifecycle::Deprecated {
            replaced_by: Some(GlossaryTermId::new("t-new")),
            deprecated_at: Utc::now(),
        };
        assert!(!t.is_active());
        assert_eq!(t.replaced_by().unwrap().as_ref(), "t-new");
    }

    #[test]
    fn valid_at_respects_open_and_closed_bounds() {
        let mut t = term("t-1", "Customer");
        let earlier: DateTime<Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let mid: DateTime<Utc> = "2025-06-01T00:00:00Z".parse().unwrap();
        let later: DateTime<Utc> = "2026-12-01T00:00:00Z".parse().unwrap();
        // Open ends → always valid.
        assert!(t.is_valid_at(mid));
        // Half-open lower bound.
        t.valid_from = Some(mid);
        assert!(!t.is_valid_at(earlier));
        assert!(t.is_valid_at(mid));
        assert!(t.is_valid_at(later));
        // Half-open upper bound (exclusive).
        t.valid_from = None;
        t.valid_to = Some(mid);
        assert!(t.is_valid_at(earlier));
        assert!(!t.is_valid_at(mid));
        assert!(!t.is_valid_at(later));
    }

    #[test]
    fn glossary_term_roundtrips_through_json() {
        let mut t = term("t-1", "Customer");
        t.aliases = vec![LocalizedText::new("Client"), LocalizedText::new("Buyer")];
        t.category = Some("business_concept".into());
        t.governance.origin = TermOrigin::DerivedFromColumn {
            table: "customers".into(),
            column: "type".into(),
        };
        t.lifecycle = TermLifecycle::Deprecated {
            replaced_by: Some(GlossaryTermId::new("t-2")),
            deprecated_at: Utc::now(),
        };
        let j = serde_json::to_value(&t).unwrap();
        let back: GlossaryTermDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, t);
    }
}

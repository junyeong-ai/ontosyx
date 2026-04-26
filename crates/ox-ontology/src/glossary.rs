//! Business-glossary terms and taxonomies.
//!
//! A glossary lets the platform record *what* a property means in
//! domain language — separately from the technical ontology shape.
//! Two properties both typed as `String` can now be distinguished as
//! "customer email" vs "support inbox" through the glossary term
//! they link to, even if the ontology's `PropertyDef` looks
//! identical across implementations.
//!
//! Minimal Phase 5-A scope:
//!
//! - `GlossaryTermDef` — the atomic unit (id, term, aliases,
//!   description, category, optional parent).
//! - `TaxonomyDef` — a named tree view over a curated subset of
//!   terms (e.g. "Industries", "Customer segments"). A term may
//!   belong to multiple taxonomies; the taxonomy names the root and
//!   the shape of the tree.
//!
//! `GlossaryTermId` is the identifier that property / node metadata
//! points at. Each `PropertyDef` gets an optional
//! `glossary_term_id: Option<GlossaryTermId>` field in Phase 5-B.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

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

    /// Canonical short name, rendered as the column label in
    /// glossary UIs. Tools treat `term` as human-readable text — it
    /// does not need to be Cypher-safe.
    pub term: String,

    /// Localized display name (same role as `GlossaryTermDef.term`
    /// but per-locale — takes precedence when the viewer's locale
    /// matches).
    #[serde(default)]
    pub display_name: LocalizedText,

    /// Free-form domain description. Longer than the term, localized
    /// so a bilingual deployment can ship English + Korean text
    /// without inventing a second glossary store.
    #[serde(default)]
    pub description: LocalizedText,

    /// Author-supplied category (e.g. `"business_concept"`,
    /// `"measure"`, `"dimension"`). No fixed taxonomy — categories
    /// are tenant-defined so the glossary doesn't force an
    /// upstream ontology.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Alternate names the term may be known as. Used for
    /// synonym-aware search in the glossary UI and for LLM prompts
    /// that need to normalise arbitrary user phrasing onto a term.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// Optional parent term — shallow hierarchy without committing
    /// to a full taxonomy. Deeper structure is expressed through
    /// `TaxonomyDef`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_term_id: Option<GlossaryTermId>,
}

impl GlossaryTermDef {
    /// Does `other` refer to the same term text (case-insensitive)
    /// when considering aliases? Useful for incremental imports that
    /// need to merge two descriptions of the same business concept
    /// without duplicating the record.
    pub fn matches_text(&self, other: &str) -> bool {
        let o = other.trim().to_lowercase();
        if self.term.to_lowercase() == o {
            return true;
        }
        self.aliases.iter().any(|a| a.to_lowercase() == o)
    }
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
    /// Walk the tree in pre-order, yielding every term id. `0.k` term
    /// navigation comes later; this is the primitive callers build on.
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
            term: text.into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            category: None,
            aliases: Vec::new(),
            parent_term_id: None,
        }
    }

    #[test]
    fn matches_text_is_case_insensitive_and_trims() {
        let mut t = term("t-1", "Customer");
        t.aliases.push("Client".into());
        assert!(t.matches_text("customer"));
        assert!(t.matches_text("  CUSTOMER "));
        assert!(t.matches_text("client"));
        assert!(!t.matches_text("vendor"));
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
    fn glossary_term_roundtrips_through_json() {
        let mut t = term("t-1", "Customer");
        t.aliases = vec!["Client".into(), "Buyer".into()];
        t.category = Some("business_concept".into());
        let j = serde_json::to_value(&t).unwrap();
        let back: GlossaryTermDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, t);
    }
}

//! Workspace-canonical business concept — identity layer above the
//! glossary's lexical instances.
//!
//! `GlossaryTermDef` was historically doing two jobs at once:
//! describing a concept's *identity* (the workspace-canonical thing
//! `NodeTypeDef.concept_id` pinned, the same regardless of the
//! locale we render its label in) and describing a *lexicalization*
//! (the Korean prefLabel "고객", the English prefLabel "Customer",
//! their alias spellings). SKOS / ISO 1087-1 / FIBO all keep these
//! distinct: a `skos:Concept` is the conceptual entity, a
//! `skos:prefLabel` is the lexical realization that points at the
//! concept. Foundry, Stardog, and TopBraid follow the same split.
//!
//! `ConceptDef` lifts the identity layer out so:
//! - The concept ID is stable across locale renames. Editing the
//!   Korean prefLabel from "고객" to "회원" no longer threatens
//!   the cross-source `customer` anchor a NodeType depends on.
//! - SKOS export becomes natural — `Concept → URI`,
//!   `GlossaryTerm → prefLabel/altLabel`.
//! - Multi-source merge (CRM "Customer" ⇄ ERP "Account") finally
//!   has a typed identity to merge *onto* instead of relying on
//!   string equality of the term label.
//! - Lifecycle (deprecate / replace_by / valid_from / valid_to)
//!   and the executable realisation (segment / function /
//!   cross-entity predicate) live with the concept, not with each
//!   of its lexicalizations.
//!
//! `OntologyIR.concepts` is the canonical concept collection.
//! `NodeTypeDef.concept_id` / `EdgeTypeDef.concept_id` declare the
//! primary concept a graph type realises, while `concept_realizations`
//! records secondary interface, classification, and analytical
//! concepts without duplicating lexical term anchors on the type.

use chrono::{DateTime, Utc};
use ox_core::i18n::LocalizedText;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::glossary::{GlossaryTermId, TermLifecycle, TermRealisation};
use crate::segment::SegmentId;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`ConceptDef`]. Workspace-unique;
    /// formatted lower-snake-case (`concept_customer`, `concept_order`)
    /// per the IR's id-grammar convention.
    ConceptId
);

/// Workspace-canonical business concept — the identity layer above
/// any lexicalization the glossary records.
///
/// Implementer pin: graph types reference the primary `ConceptDef`
/// through `concept_id` and optional additional realizations through
/// `concept_realizations`. The canonical prefLabel lives on the
/// referenced `GlossaryTermDef` (via `canonical_term_id`); aliases fan
/// out across `alias_term_ids` so a multilingual deployment can ship
/// the Korean and English term records side-by-side without inventing
/// a second concept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ConceptDef {
    /// Stable identity. Survives prefLabel rewrites, locale flips,
    /// and source-merge reconciliation. NodeType / EdgeType implementers
    /// pin this id, not the term id, so lexical churn stays local.
    pub id: ConceptId,

    /// The canonical prefLabel realisation. Points at a
    /// `GlossaryTermDef.id` whose role is `Canonical` for this
    /// concept. The GlossaryTerm itself owns the localised text.
    pub canonical_term_id: GlossaryTermId,

    /// Alternative lexicalizations — synonyms, abbreviations,
    /// per-locale variants. Each entry resolves to a
    /// `GlossaryTermDef.id` whose role is `Alias` for this concept.
    /// Empty when the concept has no alias terms registered yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alias_term_ids: Vec<GlossaryTermId>,

    /// SKOS-aligned hierarchy parent. `Some(parent_id)` declares
    /// this concept as a `skos:narrower` of `parent_id`; `None`
    /// makes it a top-of-tree entry inside its workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broader: Option<ConceptId>,

    /// Domain definition. Localised so a bilingual deployment ships
    /// English + Korean text without duplicating the concept.
    #[serde(default)]
    pub description: LocalizedText,

    /// Illustrative examples that disambiguate abstract definitions
    /// — sentence-shaped and locale-aware. SKOS `skos:example`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<LocalizedText>,

    /// Author-supplied category (`"business_concept"`, `"measure"`,
    /// `"dimension"`). Free-form; the platform never imposes a fixed
    /// taxonomy here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Optional executable spec for "what does it mean for a row
    /// to belong to this concept?" — segment membership,
    /// function-derived value, cross-entity predicate. The
    /// realisation lives on the concept (not on each
    /// lexicalization) so a translated alias term can never
    /// declare a different membership rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realisation: Option<TermRealisation>,

    /// Lifecycle state. Drives UI affordances (deprecation
    /// strikethrough, retired filter) and downstream resolvers
    /// (NL-to-Cypher should route a deprecated concept through
    /// its `replaced_by` automatically).
    #[serde(default)]
    pub lifecycle: TermLifecycle,

    /// Pointer to the successor concept when this one is deprecated.
    /// Lets the resolver chain forward without touching individual
    /// term records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<ConceptId>,

    /// Inclusive lower bound on the concept's validity window.
    /// `None` means "valid since the beginning of the ontology lineage".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,

    /// Exclusive upper bound on the concept's validity window.
    /// `None` means open-ended; a retired concept keeps `valid_to`
    /// set so historical queries can still reference it as it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,

    /// Authorship and editorial trail. Mirrors the term's
    /// `governance` shape so downstream consumers reach for the
    /// same fields regardless of which entity they're inspecting.
    #[serde(default)]
    pub governance: ConceptGovernance,
}

/// Editorial trail for a concept. Names the operator and the time
/// of the last edit so the audit dashboard can attribute a
/// definition rewrite without joining a side table.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct ConceptGovernance {
    /// Stable subject of the human or system actor that authored
    /// the concept. Matches `glossary` governance shape so audit
    /// queries can union across both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// ISO-8601 last-edited timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_edited_at: Option<DateTime<Utc>>,
}

impl ConceptDef {
    /// Pull the SegmentId out when the realisation is segment-shaped.
    /// Lets validators short-circuit the segment-existence check
    /// without a full match.
    pub fn segment_id(&self) -> Option<&SegmentId> {
        match &self.realisation {
            Some(TermRealisation::Segment { segment_id }) => Some(segment_id),
            _ => None,
        }
    }

    /// Pull the FunctionId out when the realisation is function-shaped.
    pub fn function_id(&self) -> Option<&crate::function::FunctionId> {
        match &self.realisation {
            Some(TermRealisation::Function { function_id }) => Some(function_id),
            _ => None,
        }
    }

    /// All lexicalization term ids — canonical first, then aliases
    /// in registration order. Schema RAG indexers and binding
    /// suggesters walk this iter to enumerate every term that
    /// resolves to the concept.
    pub fn lexicalization_term_ids(&self) -> impl Iterator<Item = &GlossaryTermId> {
        std::iter::once(&self.canonical_term_id).chain(self.alias_term_ids.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glossary::GlossaryTermId;

    fn term(id: &str) -> GlossaryTermId {
        GlossaryTermId::new(id)
    }

    #[test]
    fn lexicalization_iter_yields_canonical_then_aliases() {
        let c = ConceptDef {
            id: ConceptId::new("c-customer"),
            canonical_term_id: term("t-customer-en"),
            alias_term_ids: vec![term("t-customer-ko"), term("t-customer-ja")],
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
        };
        let ids: Vec<&GlossaryTermId> = c.lexicalization_term_ids().collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], &term("t-customer-en"));
        assert_eq!(ids[1], &term("t-customer-ko"));
        assert_eq!(ids[2], &term("t-customer-ja"));
    }

    #[test]
    fn segment_id_short_circuits_for_segment_realisation() {
        use crate::segment::SegmentId;
        let c = ConceptDef {
            id: ConceptId::new("c-active-customer"),
            canonical_term_id: term("t-active-customer"),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: Some(TermRealisation::Segment {
                segment_id: SegmentId::new("seg-active-customer"),
            }),
            lifecycle: TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: ConceptGovernance::default(),
        };
        assert_eq!(
            c.segment_id().map(|s| s.as_str()),
            Some("seg-active-customer"),
        );
        assert!(c.function_id().is_none());
    }
}

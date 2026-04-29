//! `ConceptDef` — workspace-canonical business concept above the
//! NodeType / EdgeType axis (ADR-0014).
//!
//! A `NodeTypeDef` is the structural shape (a row in `customers`
//! has these properties); a `ConceptDef` is the business-level
//! identity ("Customer") that one or more NodeTypes implement.
//! The split lets two sources publish their own NodeType for the
//! same canonical Customer concept — both flag `concept_id =
//! Some(customer_concept)` and downstream consumers walk the
//! reverse index to ask "every NodeType realising Customer".
//!
//! Three downstream surfaces depend on the indirection:
//!
//! - **Multi-source unification** — federated edges and homonym
//!   detection ("CRM.Customer vs ERP.Customer — same concept?")
//!   need a stable identity above the per-source label.
//! - **Glossary realisation** — a `GlossaryTermDef` is the
//!   workspace-level vocabulary entry; `ConceptDef` is the
//!   typed-realisation pointer that the runtime consumes. Anchor
//!   stays 1:1 (concept ↔ term) so the editor cannot accidentally
//!   bind two concepts to the same term.
//! - **Query-time disambiguation** — when the LLM emits "find
//!   active customers", the planner walks the concept reverse
//!   index to enumerate every NodeType that realises Customer
//!   *and* every Segment / Function the concept names as its
//!   `realisation`.
//!
//! `ConceptDef` is intentionally narrow: it carries identity +
//! anchor + optional realisation. Cross-cutting governance
//! (lifecycle, ownership, change routing) lives on the anchored
//! `GlossaryTermDef`, not duplicated here. The NodeTypes that
//! realise the concept stay the source of truth for properties,
//! constraints, and physical mapping.

use ox_core::i18n::LocalizedText;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::function::FunctionId;
use crate::glossary::GlossaryTermId;
use crate::segment::SegmentId;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`ConceptDef`].
    ConceptId
);

/// One workspace-canonical business concept.
///
/// `id` is the Concept's primary key. `glossary_term_id` is the
/// 1:1 anchor into the workspace glossary — the canonical name,
/// description, aliases, and lifecycle metadata live there. The
/// optional `realisation` lets the concept declare *how* its
/// membership is computed at query time; concepts without a
/// realisation are pure identity carriers (the implementing
/// NodeTypes carry the structural truth).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ConceptDef {
    pub id: ConceptId,
    /// Internal short name — opaque slug, not displayed. Display
    /// strings live on the anchored `GlossaryTermDef`.
    pub name: String,
    /// 1:1 anchor into the workspace glossary. `validate()`
    /// asserts every concept's term resolves and that no two
    /// concepts share a term (homonym defence).
    pub glossary_term_id: GlossaryTermId,
    #[serde(default)]
    pub description: LocalizedText,
    /// Optional executable spec for "what does it mean to belong
    /// to this concept?" — segment membership, function-derived
    /// value, cross-entity predicate. Concepts without a
    /// realisation are identity-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realisation: Option<TermRealisation>,
}

/// Executable realisation of a concept — how the runtime decides
/// membership / value at query time.
///
/// `Segment` is the canonical case: "Active Customer = Customer
/// whose last_order < 90 days" lowers to a `SegmentDef`. `Function`
/// covers computed-value concepts ("Lifetime Value = sum of order
/// totals"). `CrossEntity` is the structured-predicate escape for
/// the rare case neither shape fits — the predicate is parsed by
/// the planner against the concept's implementing NodeTypes.
///
/// `Query` (saved-view realisation) was deliberately rejected:
/// `InsightId` lives in `ox-store`, not the IR, and layering
/// `ox-ontology → ox-store` would break the dependency arrow
/// `ox-core ← ox-ontology ← ox-store`. View-shaped concepts
/// instead use a `Function` whose body returns the saved-view
/// rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TermRealisation {
    Segment {
        segment_id: SegmentId,
    },
    Function {
        function_id: FunctionId,
    },
    /// Free-form predicate evaluated against the concept's
    /// implementing NodeTypes. Surfaced to the planner as a
    /// structured filter — the predicate's properties must
    /// resolve on at least one implementer for the concept to
    /// validate.
    CrossEntity {
        predicate: String,
    },
}

impl ConceptDef {
    /// Pull the SegmentId out when the realisation is segment-
    /// shaped. Lets validators short-circuit the segment-existence
    /// check without a full match.
    pub fn segment_id(&self) -> Option<&SegmentId> {
        match &self.realisation {
            Some(TermRealisation::Segment { segment_id }) => Some(segment_id),
            _ => None,
        }
    }

    pub fn function_id(&self) -> Option<&FunctionId> {
        match &self.realisation {
            Some(TermRealisation::Function { function_id }) => Some(function_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concept(realisation: Option<TermRealisation>) -> ConceptDef {
        ConceptDef {
            id: ConceptId::new("c-customer"),
            name: "customer".to_string(),
            glossary_term_id: GlossaryTermId::new("gt-customer"),
            description: LocalizedText::default(),
            realisation,
        }
    }

    #[test]
    fn concept_round_trips_through_serde_without_realisation() {
        let c = concept(None);
        let json = serde_json::to_string(&c).unwrap();
        let back: ConceptDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn concept_with_segment_realisation_round_trips() {
        let c = concept(Some(TermRealisation::Segment {
            segment_id: SegmentId::new("seg-active"),
        }));
        let json = serde_json::to_string(&c).unwrap();
        let back: ConceptDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.segment_id().map(|s| s.as_str()), Some("seg-active"));
    }

    #[test]
    fn concept_with_function_realisation_round_trips() {
        let c = concept(Some(TermRealisation::Function {
            function_id: FunctionId::new("fn-ltv"),
        }));
        let json = serde_json::to_string(&c).unwrap();
        let back: ConceptDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.function_id().map(|f| f.as_str()), Some("fn-ltv"));
    }

    #[test]
    fn realisation_accessors_are_kind_specific() {
        // Segment realisation: function_id() must return None.
        let s = concept(Some(TermRealisation::Segment {
            segment_id: SegmentId::new("seg-1"),
        }));
        assert!(s.function_id().is_none());

        // Function realisation: segment_id() must return None.
        let f = concept(Some(TermRealisation::Function {
            function_id: FunctionId::new("fn-1"),
        }));
        assert!(f.segment_id().is_none());

        // CrossEntity realisation: both accessors return None.
        let x = concept(Some(TermRealisation::CrossEntity {
            predicate: "n.last_order > now() - interval '90 days'".to_string(),
        }));
        assert!(x.segment_id().is_none());
        assert!(x.function_id().is_none());
    }
}

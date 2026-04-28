//! Property bindings — the single resolution surface for every
//! semantic decoration a [`PropertyDef`](crate::ir::PropertyDef)
//! carries.
//!
//! Each binding pairs a property with one entry in a top-level
//! registry (value set, code system, notation pattern, value range,
//! glossary term). The shape is a tagged enum **per registry kind**
//! so each variant carries only the fields that are meaningful for
//! that target — strength applies where enforcement is meaningful,
//! `concept_map_id` only where vocabulary translation makes sense,
//! and the dedup-independent kinds (`ValueRange`, `Glossary`) drop
//! the strength axis entirely.
//!
//! Strength + temporal scope are first-class so consumers don't have
//! to invent ad-hoc conventions:
//! - **Required** — write-time validation rejects values outside the
//!   binding's domain.
//! - **Preferred** — surfaces as the recommended choice in editors;
//!   non-conforming values warn but commit.
//! - **Extensible** — the binding's domain is non-exhaustive; extra
//!   values are allowed and accumulate alongside.
//! - **Example** — illustrative only; never blocks a write.
//!
//! `valid_from` / `valid_to` let an ontology version carry the
//! historical chain of bindings that applied to a property over
//! time. Consumers that filter by an `as_of` instant
//! ([`OntologyIR::as_of`](crate::ir::OntologyIR::as_of)) see only
//! the bindings whose window covers that instant.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::code_system::CodeSystemId;
use crate::concept_map::ConceptMapId;
use crate::glossary::GlossaryTermId;
use crate::notation_pattern::NotationPatternId;
use crate::value_range::ValueRangeSetId;
use crate::value_set::ValueSetId;

/// How strongly a binding constrains write-time behaviour and how
/// editors should rank the binding's domain when offering choices.
///
/// FHIR's binding-strength axis is the model: `Required`,
/// `Extensible`, `Preferred`, `Example`. We collapse FHIR's `Example`
/// into the same enum because consumers want a single dimension when
/// reasoning about UI ranking + validation policy.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BindingStrength {
    /// Writes whose value is not in the binding's domain are
    /// rejected. Editors render the domain as the only choice.
    Required,
    /// Editors recommend the binding's domain; writes outside it
    /// surface a warning but commit. The default — chosen so a
    /// missing `strength` field never silently permits arbitrary
    /// values.
    #[default]
    Preferred,
    /// The binding's domain is non-exhaustive; out-of-domain writes
    /// commit without warning. Useful for open vocabularies where
    /// the platform should learn the long tail.
    Extensible,
    /// Illustrative reference only. Editors may surface the domain
    /// for inspiration; nothing about writes is constrained.
    Example,
}

/// One semantic constraint on a property — discriminated by the
/// registry kind it points at. Each variant carries only the
/// fields that are meaningful for that target:
///
/// - `ValueSet` / `CodeSystem` — strength + concept_map (vocabulary
///   translation makes sense)
/// - `NotationPattern` — strength only (structural format, no
///   vocabulary translation)
/// - `ValueRange` — temporal window only (the IR treats ranges as
///   classifiers, not rejectors; strength would have no enforcement)
/// - `Glossary` — temporal window only (semantic anchor; strength
///   carries no enforcement semantics for "this property realises
///   this concept")
///
/// Ordering matters in [`PropertyDef::bindings`](crate::ir::PropertyDef::bindings):
/// when two bindings would both classify a value (e.g. one notation
/// pattern is a stricter form of another), consumers honour the
/// first match. `valid_from` / `valid_to` filter the active set
/// before that ordering applies.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyBinding {
    /// Values must come from a [`ValueSetDef`](crate::value_set::ValueSetDef)
    /// expansion. The composing code systems describe the domain;
    /// the value-set decides which codes from each system the
    /// property accepts.
    ValueSet {
        id: ValueSetId,
        #[serde(default)]
        strength: BindingStrength,
        /// Concept map applied when the upstream source's vocabulary
        /// differs from the binding's canonical vocabulary. The query
        /// compiler walks `(variable, property) → concept_map_id` and
        /// rewrites literals before emitting Cypher / DataFusion
        /// expressions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concept_map_id: Option<ConceptMapId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_from: Option<DateTime<Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_to: Option<DateTime<Utc>>,
    },
    /// Values must come directly from a
    /// [`CodeSystemDef`](crate::code_system::CodeSystemDef) — useful
    /// when no narrowing value-set is needed and the whole system
    /// is in scope.
    CodeSystem {
        id: CodeSystemId,
        #[serde(default)]
        strength: BindingStrength,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concept_map_id: Option<ConceptMapId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_from: Option<DateTime<Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_to: Option<DateTime<Utc>>,
    },
    /// Values must conform to a
    /// [`NotationPatternDef`](crate::notation_pattern::NotationPatternDef)
    /// — structured identifiers like `SPRING_26_001`. Notation
    /// patterns are structural, not vocabulary-translated, so no
    /// `concept_map_id`.
    NotationPattern {
        id: NotationPatternId,
        #[serde(default)]
        strength: BindingStrength,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_from: Option<DateTime<Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_to: Option<DateTime<Utc>>,
    },
    /// Numeric values are classified against a
    /// [`ValueRangeSetDef`](crate::value_range::ValueRangeSetDef).
    /// Ranges classify, they don't reject — strength carries no
    /// enforcement semantics here, so the field is omitted entirely.
    ValueRange {
        id: ValueRangeSetId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_from: Option<DateTime<Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_to: Option<DateTime<Utc>>,
    },
    /// The property realises a business concept catalogued in the
    /// [`GlossaryTermDef`](crate::glossary::GlossaryTermDef)
    /// registry. Pure semantic anchor — no value-domain constraint,
    /// no enforcement, no strength axis. Pair with a `ValueSet` /
    /// `CodeSystem` binding when the concept also dictates the value
    /// vocabulary.
    Glossary {
        id: GlossaryTermId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_from: Option<DateTime<Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_to: Option<DateTime<Utc>>,
    },
}

/// Lightweight identity for a [`PropertyBinding`] — `(kind, id)`
/// only. Used by edit operations and dedup keys that select a
/// binding without caring about strength/concept_map/temporal
/// metadata. `PropertyBinding::handle()` projects to this shape.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyBindingHandle {
    ValueSet { id: ValueSetId },
    CodeSystem { id: CodeSystemId },
    NotationPattern { id: NotationPatternId },
    ValueRange { id: ValueRangeSetId },
    Glossary { id: GlossaryTermId },
}

impl PropertyBinding {
    /// Convenience: a `ValueSet` binding at default strength, no
    /// concept-map, no temporal window. Callers that need any of
    /// those fields populate them via struct-update syntax.
    pub fn value_set(id: ValueSetId) -> Self {
        Self::ValueSet {
            id,
            strength: BindingStrength::default(),
            concept_map_id: None,
            valid_from: None,
            valid_to: None,
        }
    }

    /// Convenience: a `CodeSystem` binding at default strength.
    pub fn code_system(id: CodeSystemId) -> Self {
        Self::CodeSystem {
            id,
            strength: BindingStrength::default(),
            concept_map_id: None,
            valid_from: None,
            valid_to: None,
        }
    }

    /// Convenience: a `NotationPattern` binding at default strength.
    pub fn notation_pattern(id: NotationPatternId) -> Self {
        Self::NotationPattern {
            id,
            strength: BindingStrength::default(),
            valid_from: None,
            valid_to: None,
        }
    }

    /// Convenience: a `ValueRange` binding (no strength axis).
    pub fn value_range(id: ValueRangeSetId) -> Self {
        Self::ValueRange {
            id,
            valid_from: None,
            valid_to: None,
        }
    }

    /// Convenience: a `Glossary` binding (no strength axis).
    pub fn glossary(id: GlossaryTermId) -> Self {
        Self::Glossary {
            id,
            valid_from: None,
            valid_to: None,
        }
    }

    /// Override strength on the variants that carry one. Variants
    /// without a strength axis (`ValueRange`, `Glossary`) return
    /// `self` unchanged — the call is a no-op rather than an error
    /// because tests and migrations sometimes apply a uniform
    /// strength across mixed-kind binding lists.
    pub fn with_strength(self, strength: BindingStrength) -> Self {
        match self {
            Self::ValueSet {
                id,
                concept_map_id,
                valid_from,
                valid_to,
                ..
            } => Self::ValueSet {
                id,
                strength,
                concept_map_id,
                valid_from,
                valid_to,
            },
            Self::CodeSystem {
                id,
                concept_map_id,
                valid_from,
                valid_to,
                ..
            } => Self::CodeSystem {
                id,
                strength,
                concept_map_id,
                valid_from,
                valid_to,
            },
            Self::NotationPattern {
                id,
                valid_from,
                valid_to,
                ..
            } => Self::NotationPattern {
                id,
                strength,
                valid_from,
                valid_to,
            },
            other @ (Self::ValueRange { .. } | Self::Glossary { .. }) => other,
        }
    }

    /// Override the inclusive-lower temporal bound on every variant.
    pub fn with_valid_from(self, t: DateTime<Utc>) -> Self {
        self.replace_window(Some(t), None)
    }

    /// Override the exclusive-upper temporal bound on every variant.
    pub fn with_valid_to(self, t: DateTime<Utc>) -> Self {
        self.replace_window(None, Some(t))
    }

    fn replace_window(
        self,
        new_from: Option<DateTime<Utc>>,
        new_to: Option<DateTime<Utc>>,
    ) -> Self {
        match self {
            Self::ValueSet {
                id,
                strength,
                concept_map_id,
                valid_from,
                valid_to,
            } => Self::ValueSet {
                id,
                strength,
                concept_map_id,
                valid_from: new_from.or(valid_from),
                valid_to: new_to.or(valid_to),
            },
            Self::CodeSystem {
                id,
                strength,
                concept_map_id,
                valid_from,
                valid_to,
            } => Self::CodeSystem {
                id,
                strength,
                concept_map_id,
                valid_from: new_from.or(valid_from),
                valid_to: new_to.or(valid_to),
            },
            Self::NotationPattern {
                id,
                strength,
                valid_from,
                valid_to,
            } => Self::NotationPattern {
                id,
                strength,
                valid_from: new_from.or(valid_from),
                valid_to: new_to.or(valid_to),
            },
            Self::ValueRange {
                id,
                valid_from,
                valid_to,
            } => Self::ValueRange {
                id,
                valid_from: new_from.or(valid_from),
                valid_to: new_to.or(valid_to),
            },
            Self::Glossary {
                id,
                valid_from,
                valid_to,
            } => Self::Glossary {
                id,
                valid_from: new_from.or(valid_from),
                valid_to: new_to.or(valid_to),
            },
        }
    }

    /// Override the concept-map on the two variants that carry one.
    /// `NotationPattern` / `ValueRange` / `Glossary` return `self`
    /// unchanged — the field has no slot on those shapes.
    pub fn with_concept_map(self, cm: ConceptMapId) -> Self {
        match self {
            Self::ValueSet {
                id,
                strength,
                valid_from,
                valid_to,
                ..
            } => Self::ValueSet {
                id,
                strength,
                concept_map_id: Some(cm),
                valid_from,
                valid_to,
            },
            Self::CodeSystem {
                id,
                strength,
                valid_from,
                valid_to,
                ..
            } => Self::CodeSystem {
                id,
                strength,
                concept_map_id: Some(cm),
                valid_from,
                valid_to,
            },
            other => other,
        }
    }

    /// Project to the lightweight `(kind, id)` selector. The match
    /// is exhaustive on variants — adding a new `PropertyBinding`
    /// variant forces an extension here, keeping the handle in lock-
    /// step with the storage shape.
    pub fn handle(&self) -> PropertyBindingHandle {
        match self {
            Self::ValueSet { id, .. } => PropertyBindingHandle::ValueSet { id: id.clone() },
            Self::CodeSystem { id, .. } => {
                PropertyBindingHandle::CodeSystem { id: id.clone() }
            }
            Self::NotationPattern { id, .. } => {
                PropertyBindingHandle::NotationPattern { id: id.clone() }
            }
            Self::ValueRange { id, .. } => {
                PropertyBindingHandle::ValueRange { id: id.clone() }
            }
            Self::Glossary { id, .. } => PropertyBindingHandle::Glossary { id: id.clone() },
        }
    }

    /// Whether the binding is in effect at the given instant. A
    /// binding with no temporal window is always in effect.
    pub fn covers(&self, at: DateTime<Utc>) -> bool {
        let (from, to) = self.window();
        if let Some(start) = from
            && at < start
        {
            return false;
        }
        if let Some(end) = to
            && at >= end
        {
            return false;
        }
        true
    }

    /// `(valid_from, valid_to)` of the binding regardless of variant.
    pub fn window(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        match self {
            Self::ValueSet {
                valid_from,
                valid_to,
                ..
            }
            | Self::CodeSystem {
                valid_from,
                valid_to,
                ..
            }
            | Self::NotationPattern {
                valid_from,
                valid_to,
                ..
            }
            | Self::ValueRange {
                valid_from,
                valid_to,
                ..
            }
            | Self::Glossary {
                valid_from,
                valid_to,
                ..
            } => (*valid_from, *valid_to),
        }
    }

    /// The strength of the binding. `Glossary` and `ValueRange`
    /// don't carry an explicit strength — both report `Preferred`
    /// (the editor default) for callers that uniformly need a value.
    /// Enforcement consumers should match on the variant directly
    /// rather than relying on this getter.
    pub fn strength(&self) -> BindingStrength {
        match self {
            Self::ValueSet { strength, .. }
            | Self::CodeSystem { strength, .. }
            | Self::NotationPattern { strength, .. } => *strength,
            Self::ValueRange { .. } | Self::Glossary { .. } => BindingStrength::Preferred,
        }
    }

    /// Optional concept map. Only `ValueSet` / `CodeSystem` carry
    /// one; the other variants always return `None`.
    pub fn concept_map_id(&self) -> Option<&ConceptMapId> {
        match self {
            Self::ValueSet { concept_map_id, .. }
            | Self::CodeSystem { concept_map_id, .. } => concept_map_id.as_ref(),
            Self::NotationPattern { .. }
            | Self::ValueRange { .. }
            | Self::Glossary { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn vs_binding() -> PropertyBinding {
        PropertyBinding::ValueSet {
            id: ValueSetId::new("vs-x"),
            strength: BindingStrength::default(),
            concept_map_id: None,
            valid_from: None,
            valid_to: None,
        }
    }

    #[test]
    fn covers_includes_open_window() {
        assert!(vs_binding().covers(Utc::now()));
    }

    #[test]
    fn covers_respects_valid_from() {
        let now = Utc::now();
        let b = PropertyBinding::ValueSet {
            id: ValueSetId::new("vs-x"),
            strength: BindingStrength::default(),
            concept_map_id: None,
            valid_from: Some(now),
            valid_to: None,
        };
        assert!(b.covers(now));
        assert!(!b.covers(now - Duration::seconds(1)));
        assert!(b.covers(now + Duration::seconds(1)));
    }

    #[test]
    fn covers_respects_valid_to_exclusive() {
        let now = Utc::now();
        let b = PropertyBinding::ValueSet {
            id: ValueSetId::new("vs-x"),
            strength: BindingStrength::default(),
            concept_map_id: None,
            valid_from: None,
            valid_to: Some(now),
        };
        assert!(!b.covers(now));
        assert!(b.covers(now - Duration::seconds(1)));
        assert!(!b.covers(now + Duration::seconds(1)));
    }

    #[test]
    fn default_strength_is_preferred() {
        assert_eq!(BindingStrength::default(), BindingStrength::Preferred);
    }

    #[test]
    fn round_trips_through_serde() {
        for b in [
            vs_binding(),
            PropertyBinding::CodeSystem {
                id: CodeSystemId::new("cs-gender"),
                strength: BindingStrength::Required,
                concept_map_id: Some(ConceptMapId::new("cm-x")),
                valid_from: None,
                valid_to: None,
            },
            PropertyBinding::NotationPattern {
                id: NotationPatternId::new("np-id"),
                strength: BindingStrength::Required,
                valid_from: None,
                valid_to: None,
            },
            PropertyBinding::ValueRange {
                id: ValueRangeSetId::new("vr-age"),
                valid_from: None,
                valid_to: None,
            },
            PropertyBinding::Glossary {
                id: GlossaryTermId::new("gt-customer"),
                valid_from: None,
                valid_to: None,
            },
        ] {
            let j = serde_json::to_value(&b).expect("serialise");
            let back: PropertyBinding = serde_json::from_value(j).expect("deserialise");
            assert_eq!(back, b);
        }
    }

    #[test]
    fn glossary_and_value_range_omit_strength_field_on_wire() {
        let b = PropertyBinding::Glossary {
            id: GlossaryTermId::new("gt-x"),
            valid_from: None,
            valid_to: None,
        };
        let j = serde_json::to_value(&b).expect("serialise");
        assert!(j.get("strength").is_none());
        assert!(j.get("concept_map_id").is_none());

        let b = PropertyBinding::ValueRange {
            id: ValueRangeSetId::new("vr-x"),
            valid_from: None,
            valid_to: None,
        };
        let j = serde_json::to_value(&b).expect("serialise");
        assert!(j.get("strength").is_none());
        assert!(j.get("concept_map_id").is_none());
    }
}

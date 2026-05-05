//! `RuleDef` — SHACL Core-aligned ontology constraint. Rust-native
//! encoding of a SHACL subset wide enough for the platform's rules
//! without shipping a full SHACL engine.
//!
//! - `RuleKind` — the shape the rule is expressed as (`NodeShape`,
//!   `PropertyShape`, `EdgeShape`, `CrossEntityShape`, `StateMachine`).
//! - `ShaclConstraint` — the supported SHACL Core constraint
//!   components (cardinality, datatype, pattern, enumeration, closed,
//!   disjoint, unique-key).
//! - `Severity` / `EnforcementKind` — orthogonal to the shape:
//!   severity controls whether a violation blocks / warns / informs;
//!   enforcement names *when* the rule runs (write-time, read-time,
//!   batch).
//! - `RuleActivationKind` — when the rule is actually live
//!   (`Always`, `OnAction`, `OnSchedule`).
//!
//! A full SHACL-SPARQL (`sh:sparql`) variant is modelled via
//! `CrossEntityShape { predicate: String }` — the string is the
//! source-dialect SQL that the planner translates to a scan, not a
//! SPARQL expression.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyType;

use crate::action::RuleId;
use crate::ir::{EdgeTypeId, NodeTypeId, PropertyId};
use crate::notation_pattern::NotationPatternId;
use crate::value_set::ValueSetId;

/// Where a `RuleDef` came from. Authored rules are first-class user
/// input the admin UI lets people edit; derived rules are synthesised
/// by the platform from another piece of the IR (typically a
/// `Required`-strength `PropertyBinding` via
/// [`crate::derived_rules::OntologyIR::derive_implicit_rules`]) and must be
/// regenerated rather than edited.
///
/// Consumers gate behaviour on this field:
/// - **Editors** disable controls for `DerivedFromBinding` rules and
///   redirect the user to the source binding instead.
/// - **Exporters** (OWL/Turtle, SHACL, etc.) skip derived rules to
///   avoid double-emitting the constraint that the binding will
///   re-derive on import.
/// - **LLM context builders** strip derived rules to keep the prompt
///   focused on user intent.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleOrigin {
    /// Authored directly by an operator. Default — the rule editor
    /// stamps this kind when a human commits a rule from the UI;
    /// missing-on-wire deserialises here.
    #[default]
    Authored,

    /// Synthesised from a `PropertyBinding`. Carries the source
    /// coordinates so consumers can navigate back to the binding.
    /// Validators forbid hand-editing — the binding is the source
    /// of truth, the rule is a projection.
    DerivedFromBinding {
        node_type_id: NodeTypeId,
        property_id: PropertyId,
    },

    /// Imported from an external SHACL shape catalogue (FHIR, FIBO,
    /// gist, internal compliance bundles). The pair `(catalog,
    /// external_id)` is enough to re-fetch / diff against the
    /// upstream artifact when the catalogue advances; a missing
    /// `external_id` means the import bundled the rule without a
    /// stable upstream key (e.g., catalogue is a flat SHACL file
    /// rather than a dereferenceable graph).
    ImportedFrom {
        catalog: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        external_id: Option<String>,
    },

    /// LLM-proposed rule that an operator subsequently accepted.
    /// `prompt_id` / `model_id` close the reproducibility loop;
    /// `accepted_at` / `accepted_by` pin the human review event so
    /// the audit trail can distinguish a confirmed proposal from a
    /// pending one (a draft proposal never persists with this
    /// origin — it lives in the proposal queue until reviewed).
    LlmProposed {
        prompt_id: String,
        model_id: String,
        accepted_at: chrono::DateTime<chrono::Utc>,
        accepted_by: String,
    },

    /// Sourced from a regulatory / legal mandate. The `jurisdiction`
    /// names the regulator (`KFTC-2024`, `EU-AI-Act-Art-9`); the
    /// optional citation URL points at the canonical document so the
    /// editor can show "this rule exists because the regulator
    /// requires it" with a click-through.
    Regulatory {
        jurisdiction: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        citation_url: Option<String>,
    },

    /// Internal business policy (data quality bar, governance
    /// commitment, SLA). `policy_id` is the workspace-level identity
    /// of the policy (commonly a wiki / notion page id); `owner` is
    /// the principal accountable for the policy's lifecycle.
    BusinessPolicy {
        policy_id: String,
        owner: String,
    },

    /// Inferred from the data itself — "this property is non-null in
    /// 99.6% of rows; propose adding a NOT-NULL invariant". Always
    /// goes through operator confirmation before landing with this
    /// origin; `confidence_bps` stays attached so a low-confidence
    /// rule can be re-confirmed when the sample window rotates.
    ///
    /// Confidence is expressed in basis points (0–10000) rather than
    /// `f64` so the IR keeps `Eq` / `Hash` cleanly — float NaN and
    /// signalling values would otherwise infect every container that
    /// needs to compare or dedup rules.
    ObservedInvariant {
        confidence_bps: u32,
        sample_size: u64,
        detected_at: chrono::DateTime<chrono::Utc>,
    },

    /// Carried forward from a prior ontology version that was
    /// retired or restructured. `previous_id` lets the audit trail
    /// stitch the rule's history across the migration boundary;
    /// `migration_note` records the human rationale.
    MigratedFrom {
        previous_id: RuleId,
        migration_note: String,
    },
}

impl RuleOrigin {
    /// Whether the editor should accept hand-edits on a rule with
    /// this origin. Derived / imported / observed rules regenerate
    /// from their source signal and must not drift from it; manual
    /// origins (Authored / LlmProposed accepted / BusinessPolicy /
    /// Regulatory / MigratedFrom) carry the operator's intent and
    /// stay editable.
    pub fn is_editable(&self) -> bool {
        match self {
            RuleOrigin::DerivedFromBinding { .. }
            | RuleOrigin::ObservedInvariant { .. }
            | RuleOrigin::ImportedFrom { .. } => false,
            RuleOrigin::Authored
            | RuleOrigin::LlmProposed { .. }
            | RuleOrigin::Regulatory { .. }
            | RuleOrigin::BusinessPolicy { .. }
            | RuleOrigin::MigratedFrom { .. } => true,
        }
    }
}

/// Ontology constraint, named and serializable.
///
/// `name` is `LocalizedText` so admin surfaces (and the SHACL
/// violation message that reaches the LLM via tool results) can
/// render the rule in the workspace's locale chain. The IR's other
/// human-facing fields (`description`, `display_name`, glossary
/// `term`) follow the same pattern; making `name` a bare `String`
/// would be the only single-language island left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct RuleDef {
    pub id: RuleId,
    pub name: LocalizedText,

    #[serde(default)]
    pub description: LocalizedText,

    /// Why this rule exists — operator-facing explanation that
    /// outlives the rule's authors. Surfaced verbatim in the rule
    /// editor and the violation diagnostic so a future engineer
    /// reading a SHACL failure understands the policy intent
    /// without spelunking commit history. Empty when the rule
    /// pre-dates the rationale field; the editor prompts to
    /// backfill on next edit.
    #[serde(default)]
    pub rationale: LocalizedText,

    pub kind: RuleKind,

    #[serde(default)]
    pub severity: Severity,

    #[serde(default)]
    pub enforcement: EnforcementKind,

    #[serde(default)]
    pub activation: RuleActivationKind,

    /// Provenance of the rule. Authored rules accept edits; derived
    /// rules are regenerated from the source binding and must not be
    /// hand-edited (the editor disables controls; exporters skip
    /// them to avoid double-emit). Defaults to `Authored` when
    /// missing on the wire.
    #[serde(default)]
    pub origin: RuleOrigin,

    /// One or more constraint components, AND'd together. Empty is
    /// syntactically valid — treated as "always passes" so the
    /// editor can save a work-in-progress rule without faking a
    /// constraint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<ShaclConstraint>,

    /// Inclusive lower bound on the rule's effective window.
    /// Validators that filter the active rule set against an `as_of`
    /// instant skip rules whose `valid_from > as_of`. `None` means
    /// "in effect from the start of the ontology version".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<chrono::DateTime<chrono::Utc>>,

    /// Exclusive upper bound on the rule's effective window. `None`
    /// means "indefinitely". Same filter contract as `valid_from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<chrono::DateTime<chrono::Utc>>,

    /// Author-supplied violation message (SHACL `sh:message`).
    /// When set, validators emit this as the diagnostic body
    /// instead of falling back to the i18n catalogue keyed by
    /// constraint kind. Rule-level (not per-constraint) by design:
    /// a rule with multiple constraints reads as a single
    /// actionable unit at the operator's grain, and the surface
    /// stays bounded as new `ShaclConstraint` variants land.
    /// Localised so a bilingual deployment can ship one rule per
    /// concept instead of inventing parallel translations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sh_message: Option<LocalizedText>,
}

impl RuleDef {
    /// Whether this rule is in effect at `at`. A rule with no
    /// temporal window is always in effect — the matching default
    /// for `OntologyIR::as_of` filtering.
    pub fn covers(&self, at: chrono::DateTime<chrono::Utc>) -> bool {
        if let Some(start) = self.valid_from
            && at < start
        {
            return false;
        }
        if let Some(end) = self.valid_to
            && at >= end
        {
            return false;
        }
        true
    }
}

/// Shape variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleKind {
    /// `sh:NodeShape` on a node type.
    NodeShape { target_node_type_id: NodeTypeId },
    /// `sh:PropertyShape` on a specific property.
    PropertyShape {
        target_node_type_id: NodeTypeId,
        target_property_id: PropertyId,
    },
    /// Ontosyx extension — constraints on an edge type (its
    /// cardinality, direction, existence). Compiles to a
    /// `NodeShape` over a synthesised edge class when exported to
    /// Turtle.
    EdgeShape { target_edge_type_id: EdgeTypeId },
    /// `sh:sparql`-style target. Expressed as a platform-native
    /// predicate because we translate to Cypher or SQL rather than
    /// SPARQL.
    CrossEntityShape {
        /// Source-dialect predicate evaluated against the scan.
        predicate: String,
    },
    /// Ontosyx extension — a state machine on a property value.
    /// Compiles to disjoint `sh:in` constraints keyed on the
    /// transition map.
    StateMachine {
        target_node_type_id: NodeTypeId,
        state_property_id: PropertyId,
        /// Allowed transitions as `(from_state, to_state)` pairs.
        /// `from_state == None` is the initial creation transition.
        transitions: Vec<StateTransition>,
    },
}

/// Named state transition inside a `StateMachine` rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct StateTransition {
    /// `None` → initial creation (no prior state).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
}

/// Stable identity for a [`ShaclConstraint`] — two constraints with
/// the same signature enforce the same intent, even when they carry
/// cosmetic differences (severity wording, custom name).
///
/// **`ShaclConstraint::signature()` returns `Option<Self>`** —
/// constraints that have not opted into signature-based dedup return
/// `None`, and the dedup pipeline never collapses two `None`-signed
/// constraints together. Bucketing unhandled kinds under a shared
/// catch-all would silently dedup unrelated intents (a `MinCount`
/// rule against a `Datatype` rule on the same property), corrupting
/// the safety-net derivation any time a future variant joins the
/// system. Opt-in is the durable contract.
///
/// Keyed by the registry id (e.g. `ValueSetId`) rather than the
/// `ConstraintTarget` field — two rules whose nominal target
/// (PropertyShape's `target_property_id`) is the same and whose
/// constraint identity matches enforce the same intent. Edge case:
/// an authored rule that points its `ConstraintTarget` at a different
/// property than the rule's nominal target falls outside this dedup
/// — derivation always emits `ConstraintTarget::Inherit`, so the
/// edge case never collides with a derived rule in practice.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstraintSignature {
    /// `sh:in` keyed by the value-set identity.
    InValueSet(ValueSetId),
    /// `sh:pattern` keyed by the notation-pattern identity.
    MatchesPattern(NotationPatternId),
    /// `sh:minCount` keyed only by the constraint kind. Two MinCount
    /// constraints on the same `(node, property)` enforce overlapping
    /// intent regardless of their `min` numbers — an authored
    /// MinCount=2 already implies MinCount=1, so the implicit
    /// nullable=false derivation must not stack on top.
    MinCount,
}

/// SHACL Core constraint component. The subset covers ~95% of
/// real-world rule usage; advanced components (`sh:and`, `sh:or`,
/// `sh:xone`, recursion rules) are not yet implemented and land
/// alongside the reasoning engine.
///
/// `ShaclConstraint` lives at the **logical** layer — the SHACL
/// validator pipeline checks each constraint at write/read time and
/// produces a structured violation report. For storage-engine
/// constraints that compile to graph-DB DDL (uniqueness, existence,
/// node keys), use [`crate::ir::NodeConstraint`] on `NodeTypeDef`
/// instead. The two surfaces deliberately do not overlap: DDL
/// constraints buy database-native enforcement at the cost of
/// portability; SHACL constraints buy expressiveness at the cost of
/// requiring the validator on the write path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShaclConstraint {
    /// `sh:minCount`.
    MinCount { target: ConstraintTarget, min: u32 },
    /// `sh:maxCount`.
    MaxCount { target: ConstraintTarget, max: u32 },
    /// `sh:datatype` — the value must be compatible with the given
    /// `PropertyType`. "Compatible" uses
    /// `PropertyType::check_compatibility_with`.
    Datatype { target: ConstraintTarget, expected: PropertyType },
    /// `sh:pattern` — structured format match via a referenced
    /// [`crate::notation_pattern::NotationPatternDef`]. Replaces the
    /// former free-form `regex: String` so that edits to a named
    /// pattern propagate to every rule that references it without
    /// copy-paste drift.
    MatchesPattern {
        target: ConstraintTarget,
        notation_pattern_id: NotationPatternId,
    },
    /// `sh:in` — enumerated allowed values via a referenced
    /// [`crate::value_set::ValueSetDef`]. Replaces the former
    /// free-form `allowed: Vec<String>` so a single source of truth
    /// (the value set) feeds admin UI, LLM prompts, and the runtime
    /// validator.
    InValueSet {
        target: ConstraintTarget,
        value_set_id: ValueSetId,
    },
    /// `sh:hasValue` — the property must contain this value.
    HasValue { target: ConstraintTarget, value: String },
    /// `sh:minInclusive`.
    MinInclusive { target: ConstraintTarget, min: f64 },
    /// `sh:maxInclusive`.
    MaxInclusive { target: ConstraintTarget, max: f64 },
    /// `sh:minLength`.
    MinLength { target: ConstraintTarget, min: u32 },
    /// `sh:maxLength`.
    MaxLength { target: ConstraintTarget, max: u32 },
    /// `sh:uniqueLang` — at most one literal per language tag.
    UniqueLang { target: ConstraintTarget },
    /// `sh:closed` — no properties outside the declared set are
    /// permitted on the node.
    Closed {
        target: ConstraintTarget,
        /// Properties the shape explicitly allows; anything outside
        /// this set plus the shape's property shapes is a violation.
        allowed_properties: Vec<PropertyKey>,
    },
    /// `sh:disjoint` — target A's value set does not overlap target B's.
    Disjoint {
        a: ConstraintTarget,
        b: ConstraintTarget,
    },
    /// Ontosyx extension: composite unique key (mirrors SQL
    /// `UNIQUE(col_a, col_b)`). SHACL proper expresses this through
    /// `sh:sparql`; we keep it first-class so the planner can
    /// emit a source-native unique index when the mapping allows.
    UniqueKey {
        target_node_type_id: NodeTypeId,
        property_keys: Vec<PropertyKey>,
    },
    /// `sh:lessThan` — the property's value must be strictly less
    /// than the value of `other_property` on the same node. Used for
    /// temporal pairs (`started_at < ended_at`) and bounded numerics
    /// (`min_value < max_value`). The runtime validator emits a
    /// Cypher predicate; this is not DDL-expressible.
    LessThan {
        target: ConstraintTarget,
        /// Sibling property the value is compared against. Both
        /// properties belong to the same node-type shape; the
        /// integrity layer rejects unknown ids.
        other_property: PropertyId,
    },
    /// `sh:equals` — the property's value must equal the value of
    /// `other_property` on the same node. Used to enforce
    /// cross-system parity (e.g. internal and external invoice
    /// numbers must match after sync).
    Equals {
        target: ConstraintTarget,
        other_property: PropertyId,
    },
    /// `sh:or` — the value must satisfy at least one of the
    /// listed constraints. Models alternation that the other
    /// SHACL primitives cannot express directly: "the address
    /// satisfies the postal-format pattern OR the international
    /// alpha-3 country pattern", "the discount field is in the
    /// percentage value-set OR matches the absolute-amount
    /// pattern". Empty `branches` is treated as a vacuously
    /// failing constraint at validation time so an authoring
    /// mistake surfaces instead of silently accepting every
    /// value.
    Or { branches: Vec<ShaclConstraint> },
}

impl ShaclConstraint {
    /// Where the constraint applies. Most variants carry a single
    /// `target: ConstraintTarget`; multi-target variants (`Disjoint`,
    /// `UniqueKey`) and node-level variants without a single
    /// `target` slot return `None`. Used by the dedup pipeline to
    /// resolve the **effective** property the constraint enforces
    /// against, taking `ConstraintTarget::Property{...}` overrides
    /// into account rather than relying on the rule's nominal
    /// `target_property_id`.
    pub fn target(&self) -> Option<&ConstraintTarget> {
        match self {
            Self::MinCount { target, .. }
            | Self::MaxCount { target, .. }
            | Self::Datatype { target, .. }
            | Self::MatchesPattern { target, .. }
            | Self::InValueSet { target, .. }
            | Self::HasValue { target, .. }
            | Self::MinInclusive { target, .. }
            | Self::MaxInclusive { target, .. }
            | Self::MinLength { target, .. }
            | Self::MaxLength { target, .. }
            | Self::UniqueLang { target }
            | Self::Closed { target, .. }
            | Self::LessThan { target, .. }
            | Self::Equals { target, .. } => Some(target),
            Self::Disjoint { .. } | Self::UniqueKey { .. } | Self::Or { .. } => None,
        }
    }

    /// Dedup signature, opt-in. See [`ConstraintSignature`] for the
    /// rationale behind `Option` rather than a catch-all variant.
    ///
    /// Adding a new `ShaclConstraint` variant forces this match to
    /// be extended (no `_ =>` arm). The new variant decides
    /// explicitly whether it participates in dedup:
    /// - return `Some(ConstraintSignature::NewKind(id))` and add the
    ///   variant to `ConstraintSignature` to opt in;
    /// - return `None` to remain dedup-independent.
    pub fn signature(&self) -> Option<ConstraintSignature> {
        match self {
            Self::InValueSet { value_set_id, .. } => {
                Some(ConstraintSignature::InValueSet(value_set_id.clone()))
            }
            Self::MatchesPattern {
                notation_pattern_id,
                ..
            } => Some(ConstraintSignature::MatchesPattern(notation_pattern_id.clone())),
            Self::MinCount { .. } => Some(ConstraintSignature::MinCount),
            Self::MaxCount { .. }
            | Self::Datatype { .. }
            | Self::HasValue { .. }
            | Self::MinInclusive { .. }
            | Self::MaxInclusive { .. }
            | Self::MinLength { .. }
            | Self::MaxLength { .. }
            | Self::UniqueLang { .. }
            | Self::Closed { .. }
            | Self::Disjoint { .. }
            | Self::UniqueKey { .. }
            | Self::LessThan { .. }
            | Self::Equals { .. }
            | Self::Or { .. } => None,
        }
    }

    /// Stable identifier ("min_count", "less_than", …) used by
    /// integrity diagnostics, dedup keys, and the agent-view UI
    /// label catalogue. Mirrors the variant's `serde` tag so the
    /// wire shape and the diagnostic surface use the same name.
    ///
    /// Adding a new variant forces an arm here, so a missing
    /// label_kind shows up at compile time, not as a `<unknown>`
    /// string at runtime.
    pub fn label_kind(&self) -> &'static str {
        match self {
            Self::MinCount { .. } => "min_count",
            Self::MaxCount { .. } => "max_count",
            Self::Datatype { .. } => "datatype",
            Self::MatchesPattern { .. } => "matches_pattern",
            Self::InValueSet { .. } => "in_value_set",
            Self::HasValue { .. } => "has_value",
            Self::MinInclusive { .. } => "min_inclusive",
            Self::MaxInclusive { .. } => "max_inclusive",
            Self::MinLength { .. } => "min_length",
            Self::MaxLength { .. } => "max_length",
            Self::UniqueLang { .. } => "unique_lang",
            Self::Closed { .. } => "closed",
            Self::Disjoint { .. } => "disjoint",
            Self::UniqueKey { .. } => "unique_key",
            Self::LessThan { .. } => "less_than",
            Self::Equals { .. } => "equals",
            Self::Or { .. } => "or",
        }
    }

    /// Every cross-collection id this constraint references — value
    /// sets, notation patterns, sibling property ids — flattened
    /// into a single iterator.
    ///
    /// The integrity pass walks this list to detect dangling refs
    /// without re-implementing per-variant unwrapping in every
    /// downstream walker. Adding a new variant that references some
    /// other entity = one new arm here; the integrity layer picks it
    /// up automatically.
    pub fn referenced_ids(&self) -> Vec<ConstraintRef<'_>> {
        match self {
            Self::InValueSet { value_set_id, .. } => {
                vec![ConstraintRef::ValueSet(value_set_id)]
            }
            Self::MatchesPattern {
                notation_pattern_id,
                ..
            } => vec![ConstraintRef::NotationPattern(notation_pattern_id)],
            Self::LessThan { other_property, .. }
            | Self::Equals { other_property, .. } => {
                vec![ConstraintRef::PropertyId(other_property)]
            }
            Self::Or { branches } => branches
                .iter()
                .flat_map(|c| c.referenced_ids())
                .collect(),
            Self::MinCount { .. }
            | Self::MaxCount { .. }
            | Self::Datatype { .. }
            | Self::HasValue { .. }
            | Self::MinInclusive { .. }
            | Self::MaxInclusive { .. }
            | Self::MinLength { .. }
            | Self::MaxLength { .. }
            | Self::UniqueLang { .. }
            | Self::Closed { .. }
            | Self::Disjoint { .. }
            | Self::UniqueKey { .. } => Vec::new(),
        }
    }
}

/// A single id reference owned by a `ShaclConstraint` — used by the
/// integrity pass to walk every ref without per-variant unwrapping.
#[derive(Debug, Clone, Copy)]
pub enum ConstraintRef<'a> {
    ValueSet(&'a ValueSetId),
    NotationPattern(&'a NotationPatternId),
    PropertyId(&'a PropertyId),
}

/// The node / property / edge a constraint component targets. Most
/// components implicitly inherit the target of their enclosing
/// `RuleDef.kind`; constraints that span multiple targets (e.g.
/// `Disjoint`) name them explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstraintTarget {
    /// Inherited from the enclosing `RuleDef.kind`. Most constraints
    /// use this.
    Inherit,
    Property {
        node_type_id: NodeTypeId,
        property_id: PropertyId,
    },
    NodeType {
        node_type_id: NodeTypeId,
    },
    EdgeLabel {
        label: GraphLabel,
    },
}

/// Violation severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Rule failure blocks the operation (write rejected, query
    /// fails, batch validation report marks the row invalid).
    #[default]
    Violation,
    /// Rule failure is surfaced as a warning. UI highlights the row;
    /// the operation itself still proceeds.
    Warning,
    /// Rule failure is purely informational — helpful for telemetry
    /// (SLA miss counters, etc.) without user-visible friction.
    Info,
}

/// When the rule runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementKind {
    /// Pre-execute check on a mutation. Failure aborts the write.
    #[default]
    Write,
    /// Post-scan check on a read. Failure surfaces as a
    /// `ResultIssue`; rows stay in the result, the caller decides
    /// how to react.
    Read,
    /// Periodic reconciliation by the data-quality scheduler. Results
    /// land in `DataQualityDef` reports, not in user request
    /// responses.
    Batch,
}

/// When the rule is live at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleActivationKind {
    /// Always active.
    #[default]
    Always,
    /// Active only while a specific action is executing. Used for
    /// `ActionDef.preconditions` / `postconditions`.
    OnAction { action_id: crate::action::ActionId },
    /// Active on a cron schedule (batch enforcement). `Batch`
    /// enforcement + `OnSchedule` activation is the natural pairing.
    OnSchedule { cron_expression: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_severity_is_violation() {
        assert_eq!(Severity::default(), Severity::Violation);
    }

    #[test]
    fn default_enforcement_is_write() {
        assert_eq!(EnforcementKind::default(), EnforcementKind::Write);
    }

    #[test]
    fn signature_distinguishes_value_sets_by_id() {
        let a = ShaclConstraint::InValueSet {
            target: ConstraintTarget::Inherit,
            value_set_id: ValueSetId::new("vs-a"),
        };
        let b = ShaclConstraint::InValueSet {
            target: ConstraintTarget::Inherit,
            value_set_id: ValueSetId::new("vs-b"),
        };
        let a_dup = ShaclConstraint::InValueSet {
            // Different `target`, same `value_set_id` — signature
            // ignores the target since dedup is intent-based, not
            // syntactic.
            target: ConstraintTarget::Property {
                node_type_id: NodeTypeId::new("nt-x"),
                property_id: PropertyId::new("p-x"),
            },
            value_set_id: ValueSetId::new("vs-a"),
        };
        assert_eq!(a.signature(), a_dup.signature());
        assert_ne!(a.signature(), b.signature());
    }

    #[test]
    fn signature_distinguishes_notation_patterns_by_id() {
        let a = ShaclConstraint::MatchesPattern {
            target: ConstraintTarget::Inherit,
            notation_pattern_id: NotationPatternId::new("np-a"),
        };
        let b = ShaclConstraint::MatchesPattern {
            target: ConstraintTarget::Inherit,
            notation_pattern_id: NotationPatternId::new("np-b"),
        };
        assert_ne!(a.signature(), b.signature());
    }

    #[test]
    fn target_returns_some_for_property_constraints() {
        let cases: [ShaclConstraint; 4] = [
            ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 1,
            },
            ShaclConstraint::InValueSet {
                target: ConstraintTarget::Property {
                    node_type_id: NodeTypeId::new("nt-x"),
                    property_id: PropertyId::new("p-x"),
                },
                value_set_id: ValueSetId::new("vs-x"),
            },
            ShaclConstraint::MatchesPattern {
                target: ConstraintTarget::Inherit,
                notation_pattern_id: NotationPatternId::new("np-x"),
            },
            ShaclConstraint::UniqueLang {
                target: ConstraintTarget::Inherit,
            },
        ];
        for c in &cases {
            assert!(
                c.target().is_some(),
                "single-target constraint must expose its target: {c:?}"
            );
        }
    }

    #[test]
    fn target_returns_none_for_multi_target_kinds() {
        // `Disjoint` carries two targets; `UniqueKey` is node-level.
        // Neither has a single `target` slot — exposing `None` keeps
        // the dedup pipeline from accidentally treating a multi-
        // target constraint as if it had a single effective property.
        let disjoint = ShaclConstraint::Disjoint {
            a: ConstraintTarget::Inherit,
            b: ConstraintTarget::Inherit,
        };
        let unique_key = ShaclConstraint::UniqueKey {
            target_node_type_id: NodeTypeId::new("nt-x"),
            property_keys: vec![],
        };
        assert!(disjoint.target().is_none());
        assert!(unique_key.target().is_none());
    }

    #[test]
    fn signature_returns_none_for_dedup_independent_kinds() {
        // Variants that have not opted into signature-based dedup
        // return `None`. Two `None` signatures must NOT collapse —
        // the dedup pipeline checks `Some(sig) == Some(sig)` only.
        // Adding finer dedup for any of these kinds is a pure-
        // additive change: extend `ConstraintSignature` and return
        // `Some(...)` from `signature()`.
        let kinds = [
            ShaclConstraint::Datatype {
                target: ConstraintTarget::Inherit,
                expected: PropertyType::String,
            },
            ShaclConstraint::HasValue {
                target: ConstraintTarget::Inherit,
                value: "x".into(),
            },
            ShaclConstraint::UniqueLang {
                target: ConstraintTarget::Inherit,
            },
        ];
        for c in &kinds {
            assert!(
                c.signature().is_none(),
                "{c:?} must remain dedup-independent until it opts in"
            );
        }
    }

    #[test]
    fn min_count_signature_collapses_regardless_of_min_value() {
        // Two MinCount constraints on the same property enforce
        // overlapping intent — authored MinCount=2 already implies
        // MinCount=1, so the signature must collapse them.
        let a = ShaclConstraint::MinCount {
            target: ConstraintTarget::Inherit,
            min: 1,
        };
        let b = ShaclConstraint::MinCount {
            target: ConstraintTarget::Inherit,
            min: 5,
        };
        assert_eq!(a.signature(), b.signature());
        assert_eq!(a.signature(), Some(ConstraintSignature::MinCount));
    }

    #[test]
    fn node_shape_rule_round_trips() {
        let r = RuleDef {
            id: RuleId::new("r-email-format"),
            name: "email_is_valid".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::PropertyShape {
                target_node_type_id: NodeTypeId::new("nt-user"),
                target_property_id: PropertyId::new("prop-email"),
            },
            severity: Severity::Violation,
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![
                ShaclConstraint::MinCount {
                    target: ConstraintTarget::Inherit,
                    min: 1,
                },
                ShaclConstraint::MatchesPattern {
                    target: ConstraintTarget::Inherit,
                    notation_pattern_id: NotationPatternId::new("np-email"),
                },
            ],
            valid_from: None,
            valid_to: None,
                    sh_message: None,
        };
        let j = serde_json::to_value(&r).unwrap();
        let back: RuleDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn state_machine_rule_preserves_transition_set() {
        let r = RuleDef {
            id: RuleId::new("r-order-state"),
            name: "order_state_machine".into(),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::StateMachine {
                target_node_type_id: NodeTypeId::new("nt-order"),
                state_property_id: PropertyId::new("prop-status"),
                transitions: vec![
                    StateTransition {
                        from: None,
                        to: "draft".into(),
                    },
                    StateTransition {
                        from: Some("draft".into()),
                        to: "submitted".into(),
                    },
                    StateTransition {
                        from: Some("submitted".into()),
                        to: "paid".into(),
                    },
                ],
            },
            severity: Severity::default(),
            enforcement: EnforcementKind::Write,
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: vec![],
            valid_from: None,
            valid_to: None,
                    sh_message: None,
        };
        let j = serde_json::to_value(&r).unwrap();
        let back: RuleDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn unique_key_is_representable_as_a_first_class_variant() {
        let c = ShaclConstraint::UniqueKey {
            target_node_type_id: NodeTypeId::new("nt-invoice"),
            property_keys: vec![
                PropertyKey::new("invoice_no").unwrap(),
                PropertyKey::new("vendor_id").unwrap(),
            ],
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"kind\":\"unique_key\""));
    }

    // -----------------------------------------------------------------
    // Cross-cutting behaviour on `ShaclConstraint`
    // -----------------------------------------------------------------

    #[test]
    fn label_kind_matches_serde_tag_for_every_variant() {
        // Every variant exposes a stable `label_kind` that mirrors
        // its serde tag. Drift between the two would make
        // diagnostics print one name while the wire emitted another.
        let cases: [(ShaclConstraint, &str); 16] = [
            (
                ShaclConstraint::MinCount {
                    target: ConstraintTarget::Inherit,
                    min: 1,
                },
                "min_count",
            ),
            (
                ShaclConstraint::MaxCount {
                    target: ConstraintTarget::Inherit,
                    max: 5,
                },
                "max_count",
            ),
            (
                ShaclConstraint::Datatype {
                    target: ConstraintTarget::Inherit,
                    expected: PropertyType::String,
                },
                "datatype",
            ),
            (
                ShaclConstraint::MatchesPattern {
                    target: ConstraintTarget::Inherit,
                    notation_pattern_id: NotationPatternId::new("np-x"),
                },
                "matches_pattern",
            ),
            (
                ShaclConstraint::InValueSet {
                    target: ConstraintTarget::Inherit,
                    value_set_id: ValueSetId::new("vs-x"),
                },
                "in_value_set",
            ),
            (
                ShaclConstraint::HasValue {
                    target: ConstraintTarget::Inherit,
                    value: "x".into(),
                },
                "has_value",
            ),
            (
                ShaclConstraint::MinInclusive {
                    target: ConstraintTarget::Inherit,
                    min: 0.0,
                },
                "min_inclusive",
            ),
            (
                ShaclConstraint::MaxInclusive {
                    target: ConstraintTarget::Inherit,
                    max: 100.0,
                },
                "max_inclusive",
            ),
            (
                ShaclConstraint::MinLength {
                    target: ConstraintTarget::Inherit,
                    min: 1,
                },
                "min_length",
            ),
            (
                ShaclConstraint::MaxLength {
                    target: ConstraintTarget::Inherit,
                    max: 64,
                },
                "max_length",
            ),
            (
                ShaclConstraint::UniqueLang {
                    target: ConstraintTarget::Inherit,
                },
                "unique_lang",
            ),
            (
                ShaclConstraint::Closed {
                    target: ConstraintTarget::Inherit,
                    allowed_properties: Vec::new(),
                },
                "closed",
            ),
            (
                ShaclConstraint::Disjoint {
                    a: ConstraintTarget::Inherit,
                    b: ConstraintTarget::Inherit,
                },
                "disjoint",
            ),
            (
                ShaclConstraint::UniqueKey {
                    target_node_type_id: NodeTypeId::new("nt-x"),
                    property_keys: Vec::new(),
                },
                "unique_key",
            ),
            (
                ShaclConstraint::LessThan {
                    target: ConstraintTarget::Inherit,
                    other_property: PropertyId::new("p-end"),
                },
                "less_than",
            ),
            (
                ShaclConstraint::Equals {
                    target: ConstraintTarget::Inherit,
                    other_property: PropertyId::new("p-end"),
                },
                "equals",
            ),
        ];
        for (c, expected) in cases {
            assert_eq!(c.label_kind(), expected, "label_kind for {c:?}");
            // Serde tag mirrors label_kind so `kind:"<label>"` round-trips.
            let j = serde_json::to_value(&c).unwrap();
            let kind = j
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("<missing>");
            assert_eq!(kind, expected, "serde kind tag for {c:?}");
        }
    }

    #[test]
    fn referenced_ids_surfaces_value_set_id() {
        let c = ShaclConstraint::InValueSet {
            target: ConstraintTarget::Inherit,
            value_set_id: ValueSetId::new("vs-status"),
        };
        let refs = c.referenced_ids();
        assert_eq!(refs.len(), 1);
        match refs[0] {
            ConstraintRef::ValueSet(id) => assert_eq!(id.as_str(), "vs-status"),
            other => panic!("expected ValueSet ref, got {other:?}"),
        }
    }

    #[test]
    fn referenced_ids_surfaces_notation_pattern_id() {
        let c = ShaclConstraint::MatchesPattern {
            target: ConstraintTarget::Inherit,
            notation_pattern_id: NotationPatternId::new("np-iso8601"),
        };
        let refs = c.referenced_ids();
        assert_eq!(refs.len(), 1);
        match refs[0] {
            ConstraintRef::NotationPattern(id) => assert_eq!(id.as_str(), "np-iso8601"),
            other => panic!("expected NotationPattern ref, got {other:?}"),
        }
    }

    #[test]
    fn referenced_ids_surfaces_property_pair_sibling() {
        let c = ShaclConstraint::LessThan {
            target: ConstraintTarget::Inherit,
            other_property: PropertyId::new("p-end"),
        };
        let refs = c.referenced_ids();
        assert_eq!(refs.len(), 1);
        match refs[0] {
            ConstraintRef::PropertyId(id) => assert_eq!(id.as_str(), "p-end"),
            other => panic!("expected PropertyId ref, got {other:?}"),
        }
    }

    #[test]
    fn referenced_ids_returns_empty_for_non_referencing_kinds() {
        let cases = [
            ShaclConstraint::MinCount {
                target: ConstraintTarget::Inherit,
                min: 1,
            },
            ShaclConstraint::HasValue {
                target: ConstraintTarget::Inherit,
                value: "x".into(),
            },
            ShaclConstraint::UniqueKey {
                target_node_type_id: NodeTypeId::new("nt-x"),
                property_keys: Vec::new(),
            },
        ];
        for c in &cases {
            assert!(
                c.referenced_ids().is_empty(),
                "{c:?} should not surface cross-collection refs"
            );
        }
    }

    #[test]
    fn rule_origin_authored_default_is_editable() {
        assert!(RuleOrigin::default().is_editable());
        assert!(RuleOrigin::Authored.is_editable());
    }

    #[test]
    fn rule_origin_derived_from_binding_is_not_editable() {
        let origin = RuleOrigin::DerivedFromBinding {
            node_type_id: NodeTypeId::new("nt-1"),
            property_id: PropertyId::new("p-1"),
        };
        assert!(
            !origin.is_editable(),
            "binding-derived rules regenerate from the binding and \
             must not drift from it"
        );
    }

    #[test]
    fn rule_origin_observed_invariant_is_not_editable() {
        let origin = RuleOrigin::ObservedInvariant {
            confidence_bps: 9_960,
            sample_size: 12_345,
            detected_at: chrono::Utc::now(),
        };
        assert!(
            !origin.is_editable(),
            "data-observed invariants re-derive from the next sample \
             window — hand-edits would be erased on the next pass"
        );
    }

    #[test]
    fn rule_origin_imported_from_is_not_editable() {
        let origin = RuleOrigin::ImportedFrom {
            catalog: "FHIR-R5".into(),
            external_id: Some("vs-1".into()),
        };
        assert!(!origin.is_editable());
    }

    #[test]
    fn rule_origin_llm_proposed_accepted_is_editable() {
        // Once the operator accepts the proposal, the rule becomes
        // theirs — they own the edits going forward.
        let origin = RuleOrigin::LlmProposed {
            prompt_id: "design.toml".into(),
            model_id: "claude-opus-4-7".into(),
            accepted_at: chrono::Utc::now(),
            accepted_by: "alice".into(),
        };
        assert!(origin.is_editable());
    }

    #[test]
    fn rule_origin_regulatory_and_business_policy_are_editable() {
        let regulatory = RuleOrigin::Regulatory {
            jurisdiction: "EU-AI-Act-Art-9".into(),
            citation_url: Some("https://eur-lex.europa.eu/...".into()),
        };
        let policy = RuleOrigin::BusinessPolicy {
            policy_id: "policy-12".into(),
            owner: "alice".into(),
        };
        assert!(regulatory.is_editable());
        assert!(policy.is_editable());
    }

    #[test]
    fn rule_origin_migrated_from_carries_history_pointer_and_is_editable() {
        let origin = RuleOrigin::MigratedFrom {
            previous_id: RuleId::new("rule-old"),
            migration_note: "renamed in v3".into(),
        };
        assert!(origin.is_editable());
    }

    #[test]
    fn rule_origin_round_trips_through_serde() {
        // Each variant must round-trip so wire-shape changes that
        // accidentally drop a field surface as a deserialise failure.
        let cases = vec![
            RuleOrigin::Authored,
            RuleOrigin::DerivedFromBinding {
                node_type_id: NodeTypeId::new("nt-1"),
                property_id: PropertyId::new("p-1"),
            },
            RuleOrigin::ImportedFrom {
                catalog: "FIBO".into(),
                external_id: None,
            },
            RuleOrigin::LlmProposed {
                prompt_id: "design.toml".into(),
                model_id: "claude-opus-4-7".into(),
                accepted_at: chrono::Utc::now(),
                accepted_by: "alice".into(),
            },
            RuleOrigin::Regulatory {
                jurisdiction: "KFTC-2024".into(),
                citation_url: None,
            },
            RuleOrigin::BusinessPolicy {
                policy_id: "p-1".into(),
                owner: "bob".into(),
            },
            RuleOrigin::ObservedInvariant {
                confidence_bps: 9_500,
                sample_size: 1_000,
                detected_at: chrono::Utc::now(),
            },
            RuleOrigin::MigratedFrom {
                previous_id: RuleId::new("rule-old"),
                migration_note: "v2→v3 split".into(),
            },
        ];
        for c in cases {
            let json = serde_json::to_string(&c).expect("serialise");
            let back: RuleOrigin = serde_json::from_str(&json).expect("round-trip");
            assert_eq!(back, c);
        }
    }

    #[test]
    fn rule_origin_missing_field_deserialises_to_authored() {
        // Absent `origin` lands as `Authored` for wire compatibility.
        let json = r#"{
            "id":"r1",
            "name":{"default":"x"},
            "kind":{"kind":"node_shape","target_node_type_id":"nt-1"}
        }"#;
        let rule: RuleDef = serde_json::from_str(json).expect("legacy parse");
        assert_eq!(rule.origin, RuleOrigin::Authored);
    }
}

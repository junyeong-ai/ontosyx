//! Concept maps — declarative translations between codes in two
//! [`crate::code_system::CodeSystemDef`]s.
//!
//! A [`ConceptMapDef`] lets a deployment declare "code `A` in
//! system `Internal:CustomerStatus` means the same thing as code
//! `ACTIVE` in system `Legacy:CRM:CustStat`" at the ontology
//! level, without embedding the translation as an ad-hoc SQL
//! expression on a per-mapping basis. Runtime consumers — the
//! query planner when two sources use different codes for the
//! same concept, the admin UI's side-by-side diff view, the LLM
//! prompt layer — all consult the same declarative table.
//!
//! ## Conceptual reference
//!
//! - **HL7 FHIR R5 ConceptMap** — `source` / `target` code system
//!   refs, `group[].element[].target[].equivalence`. Our
//!   [`Equivalence`] enum matches the FHIR `ConceptMapEquivalence`
//!   code system one-for-one.
//! - **ISO/IEC 11179 Part 6** — registry crosswalk / mapping.
//! - **W3C SKOS** — `skos:exactMatch`, `skos:closeMatch`,
//!   `skos:broadMatch`, `skos:narrowMatch`, `skos:relatedMatch`.
//!
//! ## Scope
//!
//! A single `ConceptMapDef` maps **one** source system to **one**
//! target system. Mapping through a third intermediate system is
//! expressed as two concept maps; the runtime translator walks
//! them as a graph if needed. This keeps individual definitions
//! reviewable and lets downstream tooling cache per-map lookup
//! tables without cycle detection.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::code_system::CodeSystemId;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`ConceptMapDef`].
    ConceptMapId
);

/// A declarative mapping between codes in two code systems.
///
/// Mappings are directional — `source_system_id` → `target_system_id`.
/// The reverse direction is authored as a separate `ConceptMapDef`
/// when needed; automatically inverting is unsafe when
/// [`Equivalence`] is anything other than `Equivalent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ConceptMapDef {
    pub id: ConceptMapId,

    pub name: String,

    #[serde(default)]
    pub display_name: LocalizedText,

    #[serde(default)]
    pub description: LocalizedText,

    /// Semver-ish version string — concept maps evolve as source /
    /// target systems add or retire codes.
    pub version: String,

    pub source_system_id: CodeSystemId,
    pub target_system_id: CodeSystemId,

    /// The individual code-to-code mappings. Duplicates on
    /// `source_code` are legal (a source code may map to multiple
    /// target codes — record each with its own equivalence); the
    /// translator returns all matches, the caller decides which to
    /// take.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<ConceptMapping>,
}

/// One source→target entry in a [`ConceptMapDef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ConceptMapping {
    /// Raw code string in the source system (e.g., `"A"`).
    pub source_code: String,
    /// Raw code string in the target system (e.g., `"ACTIVE"`).
    pub target_code: String,
    /// Semantic relationship between source and target.
    pub equivalence: Equivalence,
    /// Optional author note explaining the mapping. Rendered in
    /// the admin UI and surfaced via the translator when a
    /// non-equivalent mapping is chosen, so the operator can see
    /// why the codes are linked.
    #[serde(default)]
    pub comment: LocalizedText,
}

/// Semantic relationship between a source and target code.
/// Mirrors HL7 FHIR `ConceptMapEquivalence`; same semantics as W3C
/// SKOS `*Match` predicates.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Equivalence {
    /// Source and target have the same meaning. Safe to substitute
    /// either direction; runtime translators can invert
    /// automatically.
    Equivalent,
    /// Target is a more specific (narrower) concept than source.
    /// Translating source→target LOSES information.
    NarrowerThanTarget,
    /// Target is a broader (more general) concept than source.
    /// Translating source→target introduces ambiguity.
    BroaderThanTarget,
    /// Related concepts, but neither an equivalence nor a hierarchy
    /// relation. Use with care; the `comment` field should explain
    /// the relation.
    Related,
    /// Explicit "no match" — authored to prevent a future editor
    /// from mistakenly adding a mapping.
    Disjoint,
}

// ---------------------------------------------------------------------------
// Translation service
// ---------------------------------------------------------------------------

/// One translation result. `source_code` echoes the input so
/// callers that batch translate many codes can correlate
/// results back to requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    pub source_code: String,
    pub target_code: String,
    pub equivalence: Equivalence,
}

impl ConceptMapDef {
    /// Translate a single `source_code` through this map. Returns
    /// every matching entry — a code may legitimately map to
    /// multiple targets with different equivalences.
    ///
    /// Callers that need a single "best" match should filter for
    /// `Equivalence::Equivalent` first, then fall back to
    /// `NarrowerThanTarget` / `BroaderThanTarget` / `Related` by
    /// policy.
    pub fn translate(&self, source_code: &str) -> Vec<Translation> {
        self.mappings
            .iter()
            .filter(|m| m.source_code == source_code)
            .map(|m| Translation {
                source_code: m.source_code.clone(),
                target_code: m.target_code.clone(),
                equivalence: m.equivalence,
            })
            .collect()
    }

    /// Reverse translation — `target_code → source_code`.
    ///
    /// Only [`Equivalence::Equivalent`] entries are eligible. Directional
    /// or lossy relationships must be authored as their own reverse
    /// [`ConceptMapDef`] and translated through [`Self::translate`] so the
    /// ontology records the explicit operator decision.
    pub fn translate_reverse(&self, target_code: &str) -> Vec<Translation> {
        self.mappings
            .iter()
            .filter(|m| {
                m.target_code == target_code && matches!(m.equivalence, Equivalence::Equivalent)
            })
            .map(|m| Translation {
                // The returned struct's "source_code" field is the
                // side we're coming FROM — which in reverse mode is
                // the target_code in the original mapping.
                source_code: m.target_code.clone(),
                target_code: m.source_code.clone(),
                equivalence: m.equivalence,
            })
            .collect()
    }

    /// Build the reverse-direction map (`target → source`) by
    /// inverting every entry. Returns `None` when at least one
    /// entry is anything other than [`Equivalence::Equivalent`] —
    /// `NarrowerThanTarget` would have to flip to `BroaderThanTarget`
    /// (and vice-versa) to be sound, which is only true when the
    /// relation is symmetric, and `Related` / `Disjoint` cannot be
    /// flipped at all without semantic loss. Callers that need a
    /// partial reverse should iterate the entries themselves and
    /// apply an explicit policy.
    ///
    /// The returned [`ConceptMapDef`] reuses the original `id`,
    /// swaps `source_system_id` ↔ `target_system_id`, and copies
    /// `name` / `version` / `display_name` / `description` verbatim.
    /// Per-entry `comment` is preserved.
    pub fn try_inverse(&self) -> Option<ConceptMapDef> {
        if !self
            .mappings
            .iter()
            .all(|m| matches!(m.equivalence, Equivalence::Equivalent))
        {
            return None;
        }
        Some(ConceptMapDef {
            id: self.id.clone(),
            name: self.name.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            version: self.version.clone(),
            source_system_id: self.target_system_id.clone(),
            target_system_id: self.source_system_id.clone(),
            mappings: self
                .mappings
                .iter()
                .map(|m| ConceptMapping {
                    source_code: m.target_code.clone(),
                    target_code: m.source_code.clone(),
                    equivalence: m.equivalence,
                    comment: m.comment.clone(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn mapping(s: &str, t: &str, eq: Equivalence) -> ConceptMapping {
        ConceptMapping {
            source_code: s.into(),
            target_code: t.into(),
            equivalence: eq,
            comment: LocalizedText::default(),
        }
    }

    fn map_def(mappings: Vec<ConceptMapping>) -> ConceptMapDef {
        ConceptMapDef {
            id: ConceptMapId::new("cm-1"),
            name: "Internal↔Legacy".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            source_system_id: CodeSystemId::new("cs-internal"),
            target_system_id: CodeSystemId::new("cs-legacy"),
            mappings,
        }
    }

    #[test]
    fn translate_returns_equivalent_match() {
        let cm = map_def(vec![
            mapping("A", "ACTIVE", Equivalence::Equivalent),
            mapping("I", "INACTIVE", Equivalence::Equivalent),
        ]);
        let out = cm.translate("A");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_code, "ACTIVE");
        assert_eq!(out[0].equivalence, Equivalence::Equivalent);
    }

    #[test]
    fn translate_returns_empty_for_missing_source() {
        let cm = map_def(vec![mapping("A", "ACTIVE", Equivalence::Equivalent)]);
        assert!(cm.translate("X").is_empty());
    }

    #[test]
    fn translate_returns_multiple_matches_with_different_equivalences() {
        // Real-world: internal "S" (Suspended) maps to legacy
        // "HOLD" (Equivalent) and "PENDING" (NarrowerThanTarget) —
        // the caller sees both and applies policy.
        let cm = map_def(vec![
            mapping("S", "HOLD", Equivalence::Equivalent),
            mapping("S", "PENDING", Equivalence::NarrowerThanTarget),
        ]);
        let out = cm.translate("S");
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|t| t.target_code == "HOLD"));
        assert!(out.iter().any(|t| t.target_code == "PENDING"));
    }

    #[test]
    fn translate_reverse_yields_source_codes() {
        let cm = map_def(vec![mapping("A", "ACTIVE", Equivalence::Equivalent)]);
        let reverse = cm.translate_reverse("ACTIVE");
        assert_eq!(reverse.len(), 1);
        assert_eq!(reverse[0].source_code, "ACTIVE"); // coming-from side
        assert_eq!(reverse[0].target_code, "A"); // going-to side
    }

    #[test]
    fn translate_reverse_ignores_directional_and_lossy_mappings() {
        let cm = map_def(vec![
            mapping("A", "ACTIVE", Equivalence::Equivalent),
            mapping("S", "PENDING", Equivalence::NarrowerThanTarget),
            mapping("B", "BROAD", Equivalence::BroaderThanTarget),
            mapping("R", "RELATED", Equivalence::Related),
            mapping("X", "LEGACY_X", Equivalence::Disjoint),
        ]);

        assert_eq!(cm.translate_reverse("ACTIVE").len(), 1);
        assert!(cm.translate_reverse("PENDING").is_empty());
        assert!(cm.translate_reverse("BROAD").is_empty());
        assert!(cm.translate_reverse("RELATED").is_empty());
        assert!(cm.translate_reverse("LEGACY_X").is_empty());
    }

    #[test]
    fn disjoint_mapping_is_preserved_as_explicit_no_match() {
        // "X" explicitly does NOT map to "LEGACY_X". Future editors
        // should not add an equivalent mapping.
        let cm = map_def(vec![mapping("X", "LEGACY_X", Equivalence::Disjoint)]);
        let out = cm.translate("X");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].equivalence, Equivalence::Disjoint);
    }

    #[test]
    fn try_inverse_swaps_systems_and_codes_when_all_equivalent() {
        let cm = map_def(vec![
            mapping("A", "ACTIVE", Equivalence::Equivalent),
            mapping("I", "INACTIVE", Equivalence::Equivalent),
        ]);
        let inv = cm.try_inverse().expect("all-equivalent map inverts");
        assert_eq!(inv.source_system_id, cm.target_system_id);
        assert_eq!(inv.target_system_id, cm.source_system_id);
        assert_eq!(inv.mappings.len(), 2);
        let active = inv
            .mappings
            .iter()
            .find(|m| m.source_code == "ACTIVE")
            .expect("ACTIVE side present");
        assert_eq!(active.target_code, "A");
        assert_eq!(active.equivalence, Equivalence::Equivalent);
    }

    #[test]
    fn try_inverse_refuses_non_equivalent_entries() {
        // A single Narrower entry poisons the whole inversion —
        // callers must filter or apply policy explicitly.
        let cm = map_def(vec![
            mapping("A", "ACTIVE", Equivalence::Equivalent),
            mapping("S", "PENDING", Equivalence::NarrowerThanTarget),
        ]);
        assert!(cm.try_inverse().is_none());

        let cm = map_def(vec![mapping("X", "LEGACY_X", Equivalence::Disjoint)]);
        assert!(cm.try_inverse().is_none());

        let cm = map_def(vec![mapping("X", "Y", Equivalence::Related)]);
        assert!(cm.try_inverse().is_none());
    }

    #[test]
    fn try_inverse_round_trips_back_to_original() {
        let cm = map_def(vec![
            mapping("A", "ACTIVE", Equivalence::Equivalent),
            mapping("I", "INACTIVE", Equivalence::Equivalent),
        ]);
        let inv = cm.try_inverse().expect("all-equivalent");
        let back = inv.try_inverse().expect("inverse of inverse");
        assert_eq!(back, cm);
    }

    #[test]
    fn concept_map_round_trips_through_json() {
        let cm = map_def(vec![
            mapping("A", "ACTIVE", Equivalence::Equivalent),
            mapping("S", "HOLD", Equivalence::NarrowerThanTarget),
        ]);
        let j = serde_json::to_value(&cm).unwrap();
        let back: ConceptMapDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, cm);
    }
}

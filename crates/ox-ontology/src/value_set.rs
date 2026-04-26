//! Value sets — bounded subsets of one or more [`CodeSystemDef`]s
//! that together declare the allowed values for a specific context
//! (a property, a rule, an API parameter).
//!
//! A [`ValueSetDef`] references codes from any number of systems
//! through an ordered list of [`ValueSetIncludeRule`]s. The
//! runtime `expand` operation walks the rules in order, including
//! or excluding codes, and returns the resulting set.
//!
//! ## Conceptual reference
//!
//! - **HL7 FHIR R5 ValueSet.compose** — `include[]` and `exclude[]`
//!   blocks with system URIs and filters. Our
//!   [`ValueSetSelector`] matches the three common FHIR filter
//!   shapes: all-codes, explicit-codes, and is-a (descendants-of).
//! - **ISO/IEC 11179 Part 3** — a Value Domain with a Permissible
//!   Value Set and a Conceptual Domain. The `ValueSetDef` is the
//!   permissible-value subset; the `CodeSystemDef` is the source
//!   of meaning (conceptual domain).
//! - **W3C SKOS ConceptScheme** — `skos:inScheme` is expressed by
//!   the `system_id` on each include rule.
//!
//! ## Composition semantics
//!
//! Rules apply in order. Each rule is either
//! [`IncludeMode::Include`] (adds codes to the set) or
//! [`IncludeMode::Exclude`] (removes codes from the set). The
//! resulting set is the net of all includes minus all excludes.
//! Exclusion of a code that was not previously included is a
//! no-op, not an error — composition is additive by design.
//!
//! Expansion is O(N × M) where N = codes in the referenced
//! systems and M = rules; for the typical deployment (hundreds of
//! codes, single-digit rules) this is negligible. A future
//! indexed expansion path lands when a tenant hits the ceiling.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::code_system::{CodeSystemDef, CodeSystemId, CodedValue, CodedValueId};

ox_core::define_id_newtype!(
    /// Stable identifier for a [`ValueSetDef`].
    ValueSetId
);

/// A named subset of one or more [`CodeSystemDef`]s.
///
/// A `ValueSetDef` decouples "what values are allowed in this
/// context" from "what codes exist in the system". A single
/// `CodeSystemDef` may be referenced by any number of
/// `ValueSetDef`s, each carving out a different subset for a
/// different use (order-states allowed in the API vs. the subset
/// exposed in the admin UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ValueSetDef {
    pub id: ValueSetId,

    /// Short human identifier (`"OpenOrderStates"`).
    pub name: String,

    /// Localized display name.
    #[serde(default)]
    pub display_name: LocalizedText,

    /// Localized description — what's the context this set applies
    /// to, when would an engineer pick this one over a sibling.
    #[serde(default)]
    pub description: LocalizedText,

    /// Version string. Value sets evolve as business requirements
    /// change (a new status gets added, a deprecated one is
    /// dropped); `PropertyDef.value_set_id` tracks the set *id*,
    /// not the version — the admin UI shows the version so
    /// operators know which one they're looking at.
    pub version: String,

    /// Ordered composition rules. Empty composition is legal (the
    /// set is empty), but usually a bug — `validate()` surfaces it
    /// as a diagnostic, not a hard error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub composition: Vec<ValueSetIncludeRule>,
}

/// One include or exclude clause in a [`ValueSetDef::composition`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ValueSetIncludeRule {
    /// Source system. Every code this rule selects comes from
    /// this system.
    pub system_id: CodeSystemId,
    /// Which codes this rule applies to.
    pub selector: ValueSetSelector,
    /// Whether the rule includes or excludes the selected codes.
    pub mode: IncludeMode,
}

/// Which codes from a system are selected by a composition rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueSetSelector {
    /// Every code in the system. Common for "expose the whole
    /// reference data to this property" flows.
    All,
    /// Only the listed codes (by `code` string, not
    /// [`CodedValueId`] — so the value set survives re-identifying
    /// codes during a merge or import).
    Explicit { codes: Vec<String> },
    /// Every code in the hierarchy rooted at `root_id`, inclusive
    /// of the root. Only valid for hierarchical systems; a flat
    /// system with a `DescendantsOf` selector is a validation
    /// error.
    DescendantsOf { root_id: CodedValueId },
    /// Every code whose `code` string matches the given regex.
    /// Intended for pattern-defined subsets (`"^S_.*"` for
    /// "sensitive" codes) rather than for ad-hoc filtering.
    CodePattern { pattern: String },
}

/// Rule mode — include or exclude. A separate enum (rather than a
/// `bool`) so the wire form is self-documenting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IncludeMode {
    Include,
    Exclude,
}

/// Outcome of expanding a [`ValueSetDef`] against a set of
/// [`CodeSystemDef`]s — the concrete codes selected plus any
/// warnings the expansion accumulated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedValueSet<'a> {
    pub codes: Vec<&'a CodedValue>,
    pub warnings: Vec<ExpandWarning>,
}

/// Non-fatal diagnostic from [`expand_value_set`] — surfaces
/// missing systems, bad regex, etc. Collected into
/// [`ExpandedValueSet::warnings`] so the caller sees the full
/// picture instead of stopping on the first problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandWarning {
    /// Referenced system not present in the supplied systems list.
    MissingSystem(CodeSystemId),
    /// `DescendantsOf` selector named a root that doesn't exist
    /// in the referenced system.
    MissingRoot {
        system_id: CodeSystemId,
        root_id: CodedValueId,
    },
    /// `DescendantsOf` on a non-hierarchical system.
    NonHierarchicalDescendantsOf { system_id: CodeSystemId },
    /// Regex failed to compile.
    InvalidRegex { pattern: String, error: String },
}

/// Expand a [`ValueSetDef`] against the supplied systems. The
/// caller typically passes [`crate::ir::OntologyIR::code_systems`];
/// the function is pure so tests and non-IR consumers (CLI tools,
/// LLM prompt builders) can exercise it directly.
///
/// The returned [`ExpandedValueSet`] preserves insertion order
/// of includes in the composition — the first rule's codes appear
/// first in `codes`. Duplicates are filtered (by
/// [`CodedValueId`]); excludes remove earlier inclusions.
pub fn expand_value_set<'a>(
    value_set: &ValueSetDef,
    systems: &'a [CodeSystemDef],
) -> ExpandedValueSet<'a> {
    use std::collections::HashSet;

    let mut selected: Vec<&'a CodedValue> = Vec::new();
    let mut selected_ids: HashSet<&CodedValueId> = HashSet::new();
    let mut warnings: Vec<ExpandWarning> = Vec::new();

    let system_by_id: std::collections::HashMap<&CodeSystemId, &CodeSystemDef> =
        systems.iter().map(|s| (&s.id, s)).collect();

    for rule in &value_set.composition {
        let Some(system) = system_by_id.get(&rule.system_id) else {
            warnings.push(ExpandWarning::MissingSystem(rule.system_id.clone()));
            continue;
        };

        let selected_by_rule = select_from_system(system, &rule.selector, &mut warnings);

        match rule.mode {
            IncludeMode::Include => {
                for cv in selected_by_rule {
                    if selected_ids.insert(&cv.id) {
                        selected.push(cv);
                    }
                }
            }
            IncludeMode::Exclude => {
                let to_remove: HashSet<&CodedValueId> =
                    selected_by_rule.iter().map(|cv| &cv.id).collect();
                selected.retain(|cv| !to_remove.contains(&cv.id));
                selected_ids.retain(|id| !to_remove.contains(id));
            }
        }
    }

    ExpandedValueSet {
        codes: selected,
        warnings,
    }
}

fn select_from_system<'a>(
    system: &'a CodeSystemDef,
    selector: &ValueSetSelector,
    warnings: &mut Vec<ExpandWarning>,
) -> Vec<&'a CodedValue> {
    match selector {
        ValueSetSelector::All => system.codes.iter().collect(),
        ValueSetSelector::Explicit { codes } => {
            let wanted: std::collections::HashSet<&str> =
                codes.iter().map(String::as_str).collect();
            system
                .codes
                .iter()
                .filter(|cv| wanted.contains(cv.code.as_str()))
                .collect()
        }
        ValueSetSelector::DescendantsOf { root_id } => {
            if !system.hierarchical {
                warnings.push(ExpandWarning::NonHierarchicalDescendantsOf {
                    system_id: system.id.clone(),
                });
                return Vec::new();
            }
            if !system.codes.iter().any(|cv| &cv.id == root_id) {
                warnings.push(ExpandWarning::MissingRoot {
                    system_id: system.id.clone(),
                    root_id: root_id.clone(),
                });
                return Vec::new();
            }
            descendants_inclusive(system, root_id)
        }
        ValueSetSelector::CodePattern { pattern } => match regex::Regex::new(pattern) {
            Ok(re) => system.codes.iter().filter(|cv| re.is_match(&cv.code)).collect(),
            Err(e) => {
                warnings.push(ExpandWarning::InvalidRegex {
                    pattern: pattern.clone(),
                    error: e.to_string(),
                });
                Vec::new()
            }
        },
    }
}

/// Inclusive BFS over `broader_id` edges in a hierarchical system.
/// Cycles are bounded by the code count — a cycle would make the
/// ontology invalid, but we still terminate defensively.
fn descendants_inclusive<'a>(
    system: &'a CodeSystemDef,
    root_id: &CodedValueId,
) -> Vec<&'a CodedValue> {
    use std::collections::{HashSet, VecDeque};

    let mut keep: HashSet<&CodedValueId> = HashSet::new();
    keep.insert(root_id);
    // Iterate until the set stops growing (fixpoint). Each pass is
    // O(n). Bound on passes = depth of tree ≤ n, so worst-case is
    // O(n²) — fine for the expected size.
    let mut changed = true;
    let mut guard = system.codes.len() + 1;
    while changed && guard > 0 {
        changed = false;
        guard -= 1;
        for cv in &system.codes {
            if let Some(parent) = &cv.broader_id
                && keep.contains(parent)
                && keep.insert(&cv.id)
            {
                changed = true;
            }
        }
    }
    // Preserve declaration order — HashSet is unordered.
    let mut deque: VecDeque<&CodedValue> = VecDeque::new();
    for cv in &system.codes {
        if keep.contains(&cv.id) {
            deque.push_back(cv);
        }
    }
    deque.into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::code_system::CodeSystemKind;

    fn cv(id: &str, code: &str, broader: Option<&str>) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: code.into(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: vec![],
            broader_id: broader.map(CodedValueId::new),
            examples: vec![],
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn sys(id: &str, hier: bool, codes: Vec<CodedValue>) -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new(id),
            name: id.into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: hier,
            codes,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn vs(rules: Vec<ValueSetIncludeRule>) -> ValueSetDef {
        ValueSetDef {
            id: ValueSetId::new("vs-1"),
            name: "test".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: rules,
        }
    }

    #[test]
    fn all_selector_yields_every_code() {
        let system = sys(
            "cs-1",
            false,
            vec![cv("cv-a", "A", None), cv("cv-b", "B", None)],
        );
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: system.id.clone(),
            selector: ValueSetSelector::All,
            mode: IncludeMode::Include,
        }]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        let codes: Vec<_> = expanded.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, vec!["A", "B"]);
        assert!(expanded.warnings.is_empty());
    }

    #[test]
    fn explicit_selector_filters_to_named_codes_only() {
        let system = sys(
            "cs-1",
            false,
            vec![
                cv("cv-a", "A", None),
                cv("cv-b", "B", None),
                cv("cv-c", "C", None),
            ],
        );
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: system.id.clone(),
            selector: ValueSetSelector::Explicit {
                codes: vec!["A".into(), "C".into()],
            },
            mode: IncludeMode::Include,
        }]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        let codes: Vec<_> = expanded.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, vec!["A", "C"]);
    }

    #[test]
    fn descendants_of_walks_hierarchy() {
        // Tree: root → (a → a1, a2), b
        let system = sys(
            "cs-1",
            true,
            vec![
                cv("cv-root", "root", None),
                cv("cv-a", "A", Some("cv-root")),
                cv("cv-a1", "A1", Some("cv-a")),
                cv("cv-a2", "A2", Some("cv-a")),
                cv("cv-b", "B", Some("cv-root")),
            ],
        );
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: system.id.clone(),
            selector: ValueSetSelector::DescendantsOf {
                root_id: CodedValueId::new("cv-a"),
            },
            mode: IncludeMode::Include,
        }]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        let codes: Vec<_> = expanded.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, vec!["A", "A1", "A2"], "root is included");
    }

    #[test]
    fn descendants_of_on_flat_system_warns_and_returns_empty() {
        let system = sys("cs-1", false, vec![cv("cv-a", "A", None)]);
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: system.id.clone(),
            selector: ValueSetSelector::DescendantsOf {
                root_id: CodedValueId::new("cv-a"),
            },
            mode: IncludeMode::Include,
        }]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        assert!(expanded.codes.is_empty());
        assert!(matches!(
            expanded.warnings.as_slice(),
            [ExpandWarning::NonHierarchicalDescendantsOf { .. }]
        ));
    }

    #[test]
    fn code_pattern_matches_regex() {
        let system = sys(
            "cs-1",
            false,
            vec![
                cv("cv-1", "STATUS_ACTIVE", None),
                cv("cv-2", "STATUS_INACTIVE", None),
                cv("cv-3", "TYPE_A", None),
            ],
        );
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: system.id.clone(),
            selector: ValueSetSelector::CodePattern {
                pattern: "^STATUS_".into(),
            },
            mode: IncludeMode::Include,
        }]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        let codes: Vec<_> = expanded.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, vec!["STATUS_ACTIVE", "STATUS_INACTIVE"]);
    }

    #[test]
    fn exclude_removes_from_earlier_includes() {
        let system = sys(
            "cs-1",
            false,
            vec![
                cv("cv-a", "A", None),
                cv("cv-b", "B", None),
                cv("cv-c", "C", None),
            ],
        );
        let value_set = vs(vec![
            ValueSetIncludeRule {
                system_id: system.id.clone(),
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            },
            ValueSetIncludeRule {
                system_id: system.id.clone(),
                selector: ValueSetSelector::Explicit {
                    codes: vec!["B".into()],
                },
                mode: IncludeMode::Exclude,
            },
        ]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        let codes: Vec<_> = expanded.codes.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, vec!["A", "C"]);
    }

    #[test]
    fn missing_system_surfaces_as_warning_not_panic() {
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: CodeSystemId::new("cs-missing"),
            selector: ValueSetSelector::All,
            mode: IncludeMode::Include,
        }]);
        let expanded = expand_value_set(&value_set, &[]);
        assert!(expanded.codes.is_empty());
        assert!(matches!(
            expanded.warnings.as_slice(),
            [ExpandWarning::MissingSystem(_)]
        ));
    }

    #[test]
    fn invalid_regex_surfaces_as_warning() {
        let system = sys("cs-1", false, vec![cv("cv-a", "A", None)]);
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: system.id.clone(),
            selector: ValueSetSelector::CodePattern {
                pattern: "[unclosed".into(),
            },
            mode: IncludeMode::Include,
        }]);
        let systems = [system];
        let expanded = expand_value_set(&value_set, &systems);
        assert!(expanded.codes.is_empty());
        assert!(matches!(
            expanded.warnings.as_slice(),
            [ExpandWarning::InvalidRegex { .. }]
        ));
    }

    #[test]
    fn value_set_round_trips_through_json() {
        let value_set = vs(vec![ValueSetIncludeRule {
            system_id: CodeSystemId::new("cs-1"),
            selector: ValueSetSelector::Explicit {
                codes: vec!["A".into()],
            },
            mode: IncludeMode::Include,
        }]);
        let j = serde_json::to_value(&value_set).unwrap();
        let back: ValueSetDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, value_set);
    }
}

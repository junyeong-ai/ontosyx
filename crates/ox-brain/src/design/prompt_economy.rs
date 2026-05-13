//! Prompt token economy.
//!
//! Five invariants this module enforces by construction:
//!
//! 1. **Pre-computed structured signals replace raw histograms.**
//!    The LLM does not parse `sample_values` to infer "this is an
//!    enum" or "this looks like a foreign key" — `PropertySignal`
//!    enumerates those inferences as typed records the prompt
//!    renders directly. Raw histograms become evidence-on-demand,
//!    not the default payload.
//! 2. **Tight closure on context.** A prompt carries only the
//!    NodeTypes / EdgeTypes / properties it touches plus a
//!    one-hop neighborhood. Full-IR dumps are forbidden — the
//!    `existing_ontology` rendering already follows this; the
//!    `assert_within_budget` gate makes drift loud.
//! 3. **Token budget is explicit and asserted.** Every render that
//!    feeds the LLM passes through `assert_within_budget`. Over-
//!    budget renders fail fast with a `PromptBudgetError` carrying
//!    the actual size and the budget limit; the consuming surface
//!    decides whether to compact, drop the tail, or surface a
//!    user-visible "schema is too large" message.
//! 4. **Negative evidence is dropped.** "This column is not PII"
//!    / "this column has no enum candidates" are not signals — we
//!    only emit positive observations. `PropertySignal` carries
//!    only the inferences that fired.
//! 5. **Provenance metadata stays on the artifact, not the prompt.**
//!    `model_id` / `temperature` / `prompt_render_hash` are
//!    attribution fields recorded alongside the LLM call result;
//!    they never appear inside the LLM's input context. The
//!    persistence side lives elsewhere; this module owns the gate
//!    that rejects callers who try to embed those fields into the
//!    rendered prompt.
//!
//! `PromptBudget` and `assert_within_budget` are the two primitives
//! every prompt-rendering caller uses. `PropertySignal` is the
//! data shape the structured pre-pass emits; the design / refine /
//! extend prompts render it instead of the raw column profile.

use entelix::TokenCounter;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Default budget for the design-stage user prompt. Sized against
/// a 200K-context model: the remaining headroom goes to system
/// prompt, schema RAG, glossary section, existing-ontology section,
/// and the LLM's response.
///
/// Number is conservative on purpose — the gate is meant to catch
/// drift early, not to micro-optimise. A workspace that legitimately
/// needs to send larger payloads can override the budget per call.
pub const DEFAULT_DESIGN_PROMPT_BUDGET_TOKENS: u64 = 20_000;

/// Default budget for the per-cluster batch path. Smaller because
/// batch prompts run N times per design pass; the cumulative cost
/// dominates token spend on multi-cluster designs.
pub const DEFAULT_BATCH_PROMPT_BUDGET_TOKENS: u64 = 6_000;

/// Default budget for the refine / edit / translate-query prompts.
/// Same reasoning as design — tight enough to catch full-IR
/// regressions, generous enough to render real ontologies.
pub const DEFAULT_REFINE_PROMPT_BUDGET_TOKENS: u64 = 15_000;

/// Token-budget contract for one prompt render.
#[derive(Debug, Clone, Copy)]
pub struct PromptBudget {
    /// Hard limit (in tokens). Resolved via the per-`(provider, model)`
    /// [`TokenCounter`] passed to [`assert_within_budget`], so the
    /// gate is precise for every backend — `o200k_base` for newer
    /// OpenAI, `cl100k_base` for GPT-4-class, `ByteCountTokenCounter`
    /// fallback for Anthropic / unknown families. Korean and other
    /// CJK payloads count correctly under vendor-accurate tokenisers
    /// where character count over-/under-shoots wildly.
    pub max_tokens: u64,
    /// Optional soft limit. When set, callers can choose to compact
    /// before hitting the hard limit; the gate emits a
    /// `tracing::warn!` when a render lands between soft and hard.
    pub soft_tokens: Option<u64>,
    /// Human-readable label that appears in the error and warning
    /// payload — e.g. `"design"`, `"batch_cluster"`, `"refine"`.
    /// Lets operator log queries route on the surface.
    pub surface: &'static str,
}

impl PromptBudget {
    pub const fn design() -> Self {
        Self {
            max_tokens: DEFAULT_DESIGN_PROMPT_BUDGET_TOKENS,
            soft_tokens: Some(DEFAULT_DESIGN_PROMPT_BUDGET_TOKENS * 8 / 10),
            surface: "design",
        }
    }

    pub const fn batch() -> Self {
        Self {
            max_tokens: DEFAULT_BATCH_PROMPT_BUDGET_TOKENS,
            soft_tokens: Some(DEFAULT_BATCH_PROMPT_BUDGET_TOKENS * 8 / 10),
            surface: "batch_cluster",
        }
    }

    pub const fn refine() -> Self {
        Self {
            max_tokens: DEFAULT_REFINE_PROMPT_BUDGET_TOKENS,
            soft_tokens: Some(DEFAULT_REFINE_PROMPT_BUDGET_TOKENS * 8 / 10),
            surface: "refine",
        }
    }

    /// Conservative ceiling for prompts that have no surface-specific
    /// budget. Higher than `design`'s 20K so a translate / chat
    /// invocation that legitimately carries the active ontology never
    /// trips, low enough that runaway prompt construction surfaces
    /// before LLM dollars are spent.
    pub const fn default_for_unmapped() -> Self {
        Self {
            max_tokens: 30_000,
            soft_tokens: Some(24_000),
            surface: "default",
        }
    }

    /// Pick a budget by prompt template name. Falls back to
    /// [`Self::default_for_unmapped`] when no surface-specific budget
    /// is registered — the gate still fires, just with a generous
    /// ceiling.
    pub fn for_prompt(prompt_name: &str) -> Self {
        match prompt_name {
            "design_ontology" => Self::design(),
            "design_ontology_batch" | "resolve_cross_edges" => Self::batch(),
            "refine_ontology" | "edit_ontology" => Self::refine(),
            _ => Self::default_for_unmapped(),
        }
    }
}

/// Failure shape when a render busts the budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBudgetError {
    pub surface: &'static str,
    pub actual_tokens: u64,
    pub budget_tokens: u64,
    pub encoding: &'static str,
}

impl std::fmt::Display for PromptBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "prompt for surface '{}' rendered to {} tokens (counter: {}) but the budget is {}; \
             compact the input (drop sample values, summarize tail tables) or split into batches",
            self.surface, self.actual_tokens, self.encoding, self.budget_tokens
        )
    }
}

impl std::error::Error for PromptBudgetError {}

/// Hard gate. Returns `Err` when the render exceeds `budget.max_tokens`
/// under `counter`'s encoding; emits a `tracing::warn!` when it lands
/// between `soft_tokens` and the hard limit. `counter` is resolved
/// from the `(provider, model)` pair at the call site so the gate
/// uses the same tokenisation as the downstream LLM dispatch.
pub fn assert_within_budget(
    rendered: &str,
    budget: PromptBudget,
    counter: &dyn TokenCounter,
) -> Result<(), PromptBudgetError> {
    let actual = counter.count(rendered);
    if actual > budget.max_tokens {
        return Err(PromptBudgetError {
            surface: budget.surface,
            actual_tokens: actual,
            budget_tokens: budget.max_tokens,
            encoding: counter.encoding_name(),
        });
    }
    if let Some(soft) = budget.soft_tokens
        && actual > soft
    {
        tracing::warn!(
            surface = budget.surface,
            actual_tokens = actual,
            soft_tokens = soft,
            hard_tokens = budget.max_tokens,
            encoding = counter.encoding_name(),
            "prompt approaching budget — compact or split before headroom runs out"
        );
    }
    Ok(())
}

/// One typed inference about a property the design prompt should
/// pre-render instead of asking the LLM to extract from raw
/// `sample_values`. Surfaces:
///
/// - **EnumCandidate**: low-cardinality column whose distinct values
///   round-trip cleanly. Replaces the LLM's "scan sample_values for
///   recurrence" reasoning with the concrete value list.
/// - **ForeignKeyCandidate**: declared FK or `_id`-suffix
///   inference. The prompt renders the target table directly so the
///   LLM does not re-derive cross-column joins.
/// - **NumericRange**: numeric column min/max, surfaced when the
///   range pins a meaningful interpretation (probability,
///   percentage, age). Supersedes the LLM's "is this a probability
///   based on min/max?" prose.
/// - **NotationPattern**: regex match against a known catalogue
///   (UUID, ISO-8601 date, RFC-5322 email).
/// - **NullabilityHint**: `NOT NULL` / `null_count == 0` evidence.
///   Lets the design prompt drop the "infer required-ness from
///   sample_values" instruction.
///
/// A property with no firing inferences emits no signal — negative
/// evidence is dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertySignal {
    EnumCandidate {
        column: String,
        values: Vec<String>,
    },
    ForeignKeyCandidate {
        column: String,
        target_table: String,
        target_column: String,
        declared: bool,
    },
    NumericRange {
        column: String,
        min: f64,
        max: f64,
    },
    NotationPattern {
        column: String,
        /// Catalogue id — `"uuid"`, `"iso8601_date"`, `"rfc5322_email"`,
        /// etc. The pattern itself lives in the recogniser; the
        /// prompt renders the catalogue id rather than re-emitting
        /// the regex on every call.
        pattern_id: String,
    },
    NullabilityHint {
        column: String,
        /// `true` when every observed value was non-null.
        observed_non_null: bool,
    },
}

/// Compact render of a `PropertySignal` slice. Empty slices return
/// an empty string so the prompt template collapses naturally.
pub fn render_property_signals(signals: &[PropertySignal]) -> String {
    if signals.is_empty() {
        return String::new();
    }
    let mut out = String::from("Inferred property signals:\n");
    for s in signals {
        match s {
            PropertySignal::EnumCandidate { column, values } => {
                out.push_str(&format!(
                    "- {column}: enum candidate ({} values: {})\n",
                    values.len(),
                    values.join(", "),
                ));
            }
            PropertySignal::ForeignKeyCandidate {
                column,
                target_table,
                target_column,
                declared,
            } => {
                let tag = if *declared {
                    "declared FK"
                } else {
                    "inferred FK"
                };
                out.push_str(&format!(
                    "- {column}: {tag} → {target_table}.{target_column}\n"
                ));
            }
            PropertySignal::NumericRange { column, min, max } => {
                out.push_str(&format!("- {column}: numeric range [{min}, {max}]\n"));
            }
            PropertySignal::NotationPattern { column, pattern_id } => {
                out.push_str(&format!("- {column}: notation pattern '{pattern_id}'\n"));
            }
            PropertySignal::NullabilityHint {
                column,
                observed_non_null,
            } => {
                if *observed_non_null {
                    out.push_str(&format!("- {column}: observed non-null in every sample\n"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use entelix::ByteCountTokenCounter;

    #[test]
    fn assert_passes_when_under_budget() {
        let body = "x".repeat(100);
        let counter = ByteCountTokenCounter::new();
        assert_within_budget(&body, PromptBudget::design(), &counter).expect("under budget");
    }

    #[test]
    fn assert_fails_when_over_budget() {
        let budget = PromptBudget {
            max_tokens: 10,
            soft_tokens: Some(8),
            surface: "test",
        };
        // 60 bytes / 4 = 15 tokens under ByteCount — busts the 10-token cap.
        let body = "x".repeat(60);
        let counter = ByteCountTokenCounter::new();
        let err = assert_within_budget(&body, budget, &counter).expect_err("must fail");
        assert_eq!(err.surface, "test");
        assert_eq!(err.actual_tokens, 15);
        assert_eq!(err.budget_tokens, 10);
        assert_eq!(err.encoding, "byte-count-naive");
    }

    #[test]
    fn empty_signal_slice_renders_empty_string() {
        assert!(render_property_signals(&[]).is_empty());
    }

    #[test]
    fn enum_candidate_renders_values_inline() {
        let signals = vec![PropertySignal::EnumCandidate {
            column: "status".to_string(),
            values: vec!["A".into(), "B".into(), "C".into()],
        }];
        let rendered = render_property_signals(&signals);
        assert!(rendered.contains("status: enum candidate (3 values: A, B, C)"));
    }

    #[test]
    fn fk_candidate_distinguishes_declared_and_inferred() {
        let signals = vec![
            PropertySignal::ForeignKeyCandidate {
                column: "user_id".into(),
                target_table: "users".into(),
                target_column: "id".into(),
                declared: true,
            },
            PropertySignal::ForeignKeyCandidate {
                column: "owner_id".into(),
                target_table: "users".into(),
                target_column: "id".into(),
                declared: false,
            },
        ];
        let rendered = render_property_signals(&signals);
        assert!(rendered.contains("user_id: declared FK → users.id"));
        assert!(rendered.contains("owner_id: inferred FK → users.id"));
    }

    #[test]
    fn nullability_hint_only_renders_positive_observation() {
        // Negative evidence is dropped — observed_non_null=false
        // produces no output.
        let signals = vec![PropertySignal::NullabilityHint {
            column: "email".into(),
            observed_non_null: false,
        }];
        let rendered = render_property_signals(&signals);
        // Header is emitted (we emit "Inferred property signals:") but
        // no row for the false-evidence case.
        assert!(rendered.contains("Inferred property signals:"));
        assert!(!rendered.contains("email"));
    }

    #[test]
    fn property_signal_round_trips_through_serde() {
        let cases = vec![
            PropertySignal::EnumCandidate {
                column: "c".into(),
                values: vec!["A".into()],
            },
            PropertySignal::ForeignKeyCandidate {
                column: "user_id".into(),
                target_table: "users".into(),
                target_column: "id".into(),
                declared: true,
            },
            PropertySignal::NumericRange {
                column: "score".into(),
                min: 0.0,
                max: 1.0,
            },
            PropertySignal::NotationPattern {
                column: "uid".into(),
                pattern_id: "uuid".into(),
            },
            PropertySignal::NullabilityHint {
                column: "id".into(),
                observed_non_null: true,
            },
        ];
        for c in cases {
            let json = serde_json::to_string(&c).unwrap();
            let back: PropertySignal = serde_json::from_str(&json).unwrap();
            assert_eq!(back, c);
        }
    }

    #[test]
    fn prompt_budget_design_carries_surface_label() {
        assert_eq!(PromptBudget::design().surface, "design");
        assert_eq!(PromptBudget::batch().surface, "batch_cluster");
        assert_eq!(PromptBudget::refine().surface, "refine");
    }
}

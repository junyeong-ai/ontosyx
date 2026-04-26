//! Sample-driven [`NotationPatternDef`] inference.
//!
//! Given a column's distinct sample values, propose a
//! [`crate::notation_pattern::NotationPatternDef`] candidate when the
//! samples agree on a structured shape (`SPRING_26_001`,
//! `INV-2025-04231`, `KE7-XXX`, …). The proposal is heuristic —
//! pure function over the sample slice, no LLM round-trip — so the
//! admin UI can preview it instantly. A future variant may layer an
//! LLM verification pass on top, but the baseline must stand alone.
//!
//! Algorithm — character-class consensus:
//!
//! 1. Tokenise each sample on non-alphanumeric runs. The runs
//!    themselves are candidate separators.
//! 2. Identify each token's class: digits-only → Integer, letters-
//!    only (one case → fixed-case Alphanumeric, mixed-case →
//!    free-case Alphanumeric), mixed alphanumeric → Alphanumeric.
//! 3. Require **every** sample to agree on token count + per-position
//!    class + separator string. Disagreement → no proposal (None).
//! 4. Aggregate per-position widths. Digits widen min..max; letters
//!    take the modal width.
//! 5. Emit the proposed NotationPatternDef with one component per
//!    consensus position. Component names default to `c1`, `c2`, …
//!    so the UI / operator renames before applying.
//!
//! Not in scope here:
//! - **CodeFromSet components.** Requires tying a token to a value
//!   set, which needs IR-level lookup. Surfaced as a follow-up
//!   suggestion: when an Alphanumeric component matches a known
//!   value set's codes, the consumer can swap to CodeFromSet.
//! - **FreeText components.** Free text in the middle of an
//!   identifier is rare and ambiguous to parse. We only emit
//!   FreeText for the trailing token when other tokens are typed.

use ox_core::source_schema::ColumnStats;

use crate::notation_pattern::{
    NotationComponent, NotationComponentKind, NotationPatternDef, NotationPatternId,
};

/// Caller-tunable thresholds for the inference heuristic.
#[derive(Debug, Clone, Copy)]
pub struct NotationInferencePolicy {
    /// Minimum number of distinct samples required to attempt
    /// inference. With fewer samples the consensus is too noisy;
    /// the inferer returns `None` instead of a low-confidence guess.
    pub min_samples: usize,
    /// Per-position consensus threshold: every sample must agree on
    /// the position's class. We don't tolerate any drift because a
    /// single off-shape sample (`"OTHER"`) reduces the proposal's
    /// reliability to zero — better to surface no pattern than a
    /// wrong one.
    pub require_full_agreement: bool,
}

impl Default for NotationInferencePolicy {
    fn default() -> Self {
        Self {
            min_samples: 3,
            require_full_agreement: true,
        }
    }
}

/// One proposed pattern + the evidence backing it. Confidence rides
/// the *fraction* of samples whose tokenisation matched the
/// consensus — `1.0` when every sample agreed, lower if drift was
/// observed (currently we only surface 1.0 because the strict
/// `require_full_agreement` policy returns `None` on any drift,
/// but the field is in place for future relaxed modes).
#[derive(Debug, Clone, PartialEq)]
pub struct NotationPatternProposal {
    pub pattern: NotationPatternDef,
    /// Up to 5 representative samples that match the proposal,
    /// surfaced verbatim so the admin UI can render real-world
    /// examples next to the proposal.
    pub examples: Vec<String>,
    pub confidence: f64,
}

/// Why inference declined to propose a pattern. Surfaced so the UI
/// can render "we looked but couldn't agree on a shape" instead of
/// silently leaving the property unannotated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotationInferenceRejection {
    /// Fewer than `policy.min_samples` distinct samples were
    /// available.
    InsufficientSamples { available: usize, required: usize },
    /// Samples disagreed on token count after tokenisation.
    TokenCountMismatch { observed_counts: Vec<usize> },
    /// Samples disagreed on per-position class.
    ClassDisagreement { position: usize, observed: Vec<String> },
    /// Samples disagreed on the separator characters.
    SeparatorDisagreement { observed: Vec<String> },
    /// Tokenised to a single free-text run — no structured
    /// components to extract.
    Unstructured,
}

/// Top-level inference entrypoint. Returns either a proposal
/// (caller decides whether to apply) or a typed rejection so the UI
/// can communicate the absence of a pattern with the actual reason.
pub fn propose_notation_pattern(
    stats: &ColumnStats,
    policy: NotationInferencePolicy,
) -> Result<NotationPatternProposal, NotationInferenceRejection> {
    let samples: Vec<&str> = stats
        .sample_values
        .iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();

    if samples.len() < policy.min_samples {
        return Err(NotationInferenceRejection::InsufficientSamples {
            available: samples.len(),
            required: policy.min_samples,
        });
    }

    // Tokenise every sample.
    let tokenisations: Vec<Tokenisation> = samples
        .iter()
        .map(|s| tokenise(s))
        .collect();

    // Token-count consensus.
    let token_counts: Vec<usize> = tokenisations.iter().map(|t| t.tokens.len()).collect();
    if !token_counts.iter().all(|n| *n == token_counts[0]) {
        return Err(NotationInferenceRejection::TokenCountMismatch {
            observed_counts: token_counts,
        });
    }
    let n_tokens = token_counts[0];
    if n_tokens < 2 {
        // A single token gives no structure — not a notation pattern.
        return Err(NotationInferenceRejection::Unstructured);
    }

    // Separator consensus — every gap between tokens must agree on
    // the separator string across samples.
    let n_gaps = n_tokens.saturating_sub(1);
    for gap_idx in 0..n_gaps {
        let observed: Vec<String> = tokenisations
            .iter()
            .map(|t| t.separators[gap_idx].clone())
            .collect();
        if !observed.iter().all(|s| s == &observed[0]) {
            return Err(NotationInferenceRejection::SeparatorDisagreement {
                observed,
            });
        }
    }
    let separator = tokenisations[0]
        .separators
        .first()
        .cloned()
        .unwrap_or_default();

    // Per-position class consensus.
    let mut consensus_kinds: Vec<NotationComponentKind> = Vec::with_capacity(n_tokens);
    for pos in 0..n_tokens {
        let classes: Vec<TokenClass> = tokenisations
            .iter()
            .map(|t| classify(&t.tokens[pos]))
            .collect();
        let head = classes[0];
        if !classes.iter().all(|c| *c == head) {
            return Err(NotationInferenceRejection::ClassDisagreement {
                position: pos,
                observed: classes
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect(),
            });
        }
        let position_tokens: Vec<&str> =
            tokenisations.iter().map(|t| t.tokens[pos].as_str()).collect();
        consensus_kinds.push(consensus_kind_for(head, &position_tokens));
    }

    // Build NotationPatternDef + template string.
    let template = build_template(&consensus_kinds, &separator);
    let components: Vec<NotationComponent> = consensus_kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| NotationComponent {
            name: format!("c{}", i + 1),
            display: Default::default(),
            kind,
        })
        .collect();

    let examples: Vec<String> = samples
        .iter()
        .take(5)
        .map(|s| (*s).to_string())
        .collect();

    let pattern_id = NotationPatternId::from(format!(
        "np_auto_{}",
        short_hash(samples[0]),
    ));

    Ok(NotationPatternProposal {
        pattern: NotationPatternDef {
            id: pattern_id,
            name: format!("AutoNotation_{}", components.len()),
            display_name: Default::default(),
            description: Default::default(),
            template,
            separator,
            components,
            examples: examples.clone(),
        },
        examples,
        confidence: 1.0,
    })
}

// ---------------------------------------------------------------------------
// Internals — tokenisation + classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Tokenisation {
    /// Alphanumeric runs in left-to-right order.
    tokens: Vec<String>,
    /// Non-alphanumeric runs that separated the tokens. `separators[i]`
    /// is the gap between `tokens[i]` and `tokens[i+1]`.
    separators: Vec<String>,
}

fn tokenise(s: &str) -> Tokenisation {
    let mut tokens = Vec::new();
    let mut separators = Vec::new();
    let mut current_token = String::new();
    let mut current_sep = String::new();
    let mut in_token = true;

    for c in s.chars() {
        if c.is_alphanumeric() {
            if !in_token && !current_sep.is_empty() {
                separators.push(std::mem::take(&mut current_sep));
                in_token = true;
            }
            current_token.push(c);
        } else {
            if in_token && !current_token.is_empty() {
                tokens.push(std::mem::take(&mut current_token));
                in_token = false;
            }
            current_sep.push(c);
        }
    }
    if !current_token.is_empty() {
        tokens.push(current_token);
    }
    // Trailing separator (if any) is dropped — it doesn't sit
    // between two tokens, so it can't be a separator in the pattern.

    Tokenisation { tokens, separators }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenClass {
    /// Digits only.
    Integer,
    /// Letters only — uppercase.
    AlphaUpper,
    /// Letters only — lowercase.
    AlphaLower,
    /// Letters only — mixed case.
    AlphaMixed,
    /// Mixed alphanumeric (letters + digits).
    Alphanumeric,
}

fn classify(token: &str) -> TokenClass {
    let mut has_digit = false;
    let mut has_upper = false;
    let mut has_lower = false;
    for c in token.chars() {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if c.is_ascii_uppercase() {
            has_upper = true;
        } else if c.is_ascii_lowercase() {
            has_lower = true;
        }
    }
    match (has_digit, has_upper, has_lower) {
        (true, false, false) => TokenClass::Integer,
        (false, true, false) => TokenClass::AlphaUpper,
        (false, false, true) => TokenClass::AlphaLower,
        (false, true, true) => TokenClass::AlphaMixed,
        _ => TokenClass::Alphanumeric,
    }
}

fn consensus_kind_for(class: TokenClass, samples: &[&str]) -> NotationComponentKind {
    match class {
        TokenClass::Integer => {
            // Width consensus for digits: every sample at this
            // position must be the same width to declare a fixed
            // width; otherwise width=0 (variable).
            let widths: Vec<usize> = samples.iter().map(|s| s.chars().count()).collect();
            let width: u8 = if widths.iter().all(|w| *w == widths[0]) {
                widths[0].min(u8::MAX as usize) as u8
            } else {
                0
            };
            // Range from observed values — bounded by min/max parsed
            // as i64 (sample tokens are digits, so always parseable).
            let parsed: Vec<i64> = samples
                .iter()
                .filter_map(|s| s.parse::<i64>().ok())
                .collect();
            let min = parsed.iter().min().copied().unwrap_or(0);
            let max = parsed.iter().max().copied().unwrap_or(0);
            NotationComponentKind::IntegerRange { min, max, width }
        }
        TokenClass::AlphaUpper
        | TokenClass::AlphaLower
        | TokenClass::AlphaMixed
        | TokenClass::Alphanumeric => {
            let widths: Vec<usize> = samples.iter().map(|s| s.chars().count()).collect();
            let width = if widths.iter().all(|w| *w == widths[0]) {
                widths[0].min(u32::MAX as usize) as u32
            } else {
                // Variable width — fall back to FreeText with
                // observed max length as the cap. This is rare; the
                // strict policy usually catches it earlier.
                let max_len = widths.iter().max().copied().unwrap_or(0) as u32;
                return NotationComponentKind::FreeText {
                    max_len: Some(max_len),
                };
            };
            let uppercase = matches!(class, TokenClass::AlphaUpper);
            NotationComponentKind::Alphanumeric { width, uppercase }
        }
    }
}

fn build_template(kinds: &[NotationComponentKind], separator: &str) -> String {
    let parts: Vec<String> = kinds
        .iter()
        .enumerate()
        .map(|(i, k)| match k {
            NotationComponentKind::IntegerRange { width, .. } if *width > 0 => {
                format!("{{c{}:%0{}d}}", i + 1, width)
            }
            _ => format!("{{c{}}}", i + 1),
        })
        .collect();
    parts.join(separator)
}

fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(samples: &[&str]) -> ColumnStats {
        ColumnStats {
            column_name: "code".into(),
            null_count: 0,
            distinct_count: samples.len() as u64,
            sample_values: samples.iter().map(|s| (*s).to_string()).collect(),
            min_value: None,
            max_value: None,
        }
    }

    #[test]
    fn proposes_pattern_for_consistent_dash_separated_codes() {
        let s = stats(&["INV-2025-001", "INV-2025-002", "INV-2024-099"]);
        let p = propose_notation_pattern(&s, NotationInferencePolicy::default())
            .expect("expected proposal");
        assert_eq!(p.pattern.separator, "-");
        assert_eq!(p.pattern.components.len(), 3);
        // INV at position 0 — uppercase 3-letter alpha.
        match &p.pattern.components[0].kind {
            NotationComponentKind::Alphanumeric { width, uppercase } => {
                assert_eq!(*width, 3);
                assert!(*uppercase);
            }
            other => panic!("expected Alphanumeric for position 0, got {other:?}"),
        }
        // 2024/2025 at position 1 — 4-digit integer in observed range.
        match &p.pattern.components[1].kind {
            NotationComponentKind::IntegerRange { min, max, width } => {
                assert_eq!(*min, 2024);
                assert_eq!(*max, 2025);
                assert_eq!(*width, 4);
            }
            other => panic!("expected IntegerRange for position 1, got {other:?}"),
        }
        // sequence at position 2 — 3-digit zero-padded.
        match &p.pattern.components[2].kind {
            NotationComponentKind::IntegerRange { width, .. } => {
                assert_eq!(*width, 3);
            }
            other => panic!("expected IntegerRange for position 2, got {other:?}"),
        }
        assert_eq!(p.confidence, 1.0);
    }

    #[test]
    fn rejects_when_token_count_disagrees() {
        let s = stats(&["A-1-2", "A-1", "A-1-2"]);
        let err = propose_notation_pattern(&s, Default::default()).unwrap_err();
        match err {
            NotationInferenceRejection::TokenCountMismatch { observed_counts } => {
                assert_eq!(observed_counts, vec![3, 2, 3]);
            }
            other => panic!("expected TokenCountMismatch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_when_separator_disagrees() {
        let s = stats(&["A-1", "A_1", "A-2"]);
        let err = propose_notation_pattern(&s, Default::default()).unwrap_err();
        assert!(matches!(
            err,
            NotationInferenceRejection::SeparatorDisagreement { .. }
        ));
    }

    #[test]
    fn rejects_when_class_disagrees_at_position() {
        // Position 0: A vs 1 — Alpha vs Integer.
        let s = stats(&["A-100", "1-100", "A-200"]);
        let err = propose_notation_pattern(&s, Default::default()).unwrap_err();
        assert!(matches!(
            err,
            NotationInferenceRejection::ClassDisagreement { position: 0, .. }
        ));
    }

    #[test]
    fn rejects_unstructured_single_token() {
        let s = stats(&["hello", "world", "again"]);
        let err = propose_notation_pattern(&s, Default::default()).unwrap_err();
        assert!(matches!(err, NotationInferenceRejection::Unstructured));
    }

    #[test]
    fn rejects_when_too_few_samples() {
        let s = stats(&["A-1"]);
        let err = propose_notation_pattern(&s, Default::default()).unwrap_err();
        match err {
            NotationInferenceRejection::InsufficientSamples {
                available,
                required,
            } => {
                assert_eq!(available, 1);
                assert_eq!(required, 3);
            }
            other => panic!("expected InsufficientSamples, got {other:?}"),
        }
    }

    #[test]
    fn template_renders_with_padded_widths() {
        let s = stats(&["INV-2025-001", "INV-2025-002", "INV-2024-099"]);
        let p = propose_notation_pattern(&s, Default::default()).expect("proposal");
        assert_eq!(p.pattern.template, "{c1}-{c2:%04d}-{c3:%03d}");
    }

    #[test]
    fn empty_separator_for_concatenated_tokens() {
        // No separator → tokens run together. Note: same class
        // (digits-only) collapses into a single token because
        // there's no non-alphanumeric break between them, so this is
        // a single-token rejection. We use mixed classes to force
        // two tokens.
        let s = stats(&["A1", "A2", "A99"]);
        // After tokenisation, "A1" → ["A1"] (single token, mixed
        // alphanumeric). So Unstructured fires.
        let err = propose_notation_pattern(&s, Default::default()).unwrap_err();
        assert!(matches!(err, NotationInferenceRejection::Unstructured));
    }
}

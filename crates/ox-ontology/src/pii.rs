//! PII redaction + suggestion primitives.
//!
//! ## Two surfaces, intentionally split
//!
//! 1. **Redaction** ([`redact_value`] / [`redact_column_stats`]) is
//!    deterministic and applies *only* when a property has been
//!    explicitly annotated with a [`crate::ir::PiiKind`]. The
//!    annotation is a user decision recorded on
//!    [`crate::ir::PropertyDef::pii_kind`]; this module never
//!    decides on its own that a value is sensitive.
//! 2. **Suggestion** ([`PiiClassifier`] / [`RegexPiiClassifier`]) is
//!    a heuristic that helps the ontology editor *propose* a
//!    `pii_kind` to the operator. It returns
//!    [`PiiSuggestion`] (kind + confidence + reason) — never
//!    a redaction. Regex-on-column-name suffers known false
//!    positives (`email_template_id` matches `email`) and false
//!    negatives (`e_addr` does not match `email`); using the same
//!    classifier to drive automatic redaction would smuggle either
//!    failure mode into the redacted output.
//!
//! ## Redaction policy (user-facing strings)
//!
//! - **Email** — preserve the first 3 chars of the local part and the
//!   last domain label; everything else becomes `***`.
//!   `john.doe@gmail.com` ⇒ `joh***@***.com`.
//! - **Phone / national-id / payment-card / bank-account / IBAN** —
//!   preserve the trailing 4 chars; rest replaced with `***`.
//!   `1234-5678-9012-3456` ⇒ `***3456`.
//! - **Password / Token / Custom and any other unhandled kind** —
//!   full replacement with the literal `<redacted>`.
//!
//! Counts (`null_count`, `distinct_count`) are *never* redacted —
//! the count itself is not PII and operators rely on the
//! distribution to detect schema drift.

use serde::{Deserialize, Serialize};

use ox_core::source_schema::ColumnStats;

use crate::ir::PiiKind;

/// Placeholder substituted for a fully-redacted value (Password,
/// Token, Custom, and any future kinds without a partial-redaction
/// rule). Pinned as a `pub const` so wire-shape clients can
/// case-fold against the same string.
pub const REDACTED_PLACEHOLDER: &str = "<redacted>";

/// Redact a single value according to its `kind`. Pure string
/// transformation — no allocation when the input is already a
/// "<redacted>" placeholder of the right shape.
pub fn redact_value(value: &str, kind: &PiiKind) -> String {
    match kind {
        PiiKind::Email => redact_email(value),
        // Numeric-tail kinds: phone, national id (KR RRN, US SSN, JP
        // My Number, …), payment card, bank account, IBAN. Each
        // benefits from the trailing-4 affordance for support
        // workflows ("the card ending in 3456") without exposing
        // the full identifier.
        PiiKind::Phone
        | PiiKind::Ssn
        | PiiKind::NationalId { .. }
        | PiiKind::PaymentCardNumber
        | PiiKind::CreditCard
        | PiiKind::BankAccountNumber
        | PiiKind::Iban
        | PiiKind::Passport
        | PiiKind::DriversLicense
        | PiiKind::MedicalRecordNumber
        | PiiKind::InsuranceId => redact_last4(value),
        PiiKind::Password
        | PiiKind::Token
        | PiiKind::Custom(_)
        | PiiKind::Name
        | PiiKind::DateOfBirth
        | PiiKind::Address
        | PiiKind::IpAddress
        | PiiKind::Biometric
        | PiiKind::GeoLocation => REDACTED_PLACEHOLDER.to_string(),
    }
}

/// Redact every sample value in `stats` according to `kind`. The
/// `min_value` / `max_value` strings are also redacted — both can
/// leak the same identifier shape that `sample_values` does
/// (`MIN(card_number) = '1000-...'`). Counts are preserved.
pub fn redact_column_stats(stats: &mut ColumnStats, kind: &PiiKind) {
    for v in &mut stats.sample_values {
        *v = redact_value(v, kind);
    }
    if let Some(min) = stats.min_value.as_mut() {
        *min = redact_value(min, kind);
    }
    if let Some(max) = stats.max_value.as_mut() {
        *max = redact_value(max, kind);
    }
}

/// Email redaction: first 3 chars of local + `***`, then `***` +
/// last domain segment (TLD). Inputs that fail to parse as an
/// email-shaped string fall back to the full-replacement placeholder
/// — better to over-redact than to leak a partial identifier
/// because the format was unexpected.
pub fn redact_email(s: &str) -> String {
    let Some(at) = s.rfind('@') else {
        return REDACTED_PLACEHOLDER.to_string();
    };
    let (local, rest) = s.split_at(at);
    let domain = &rest[1..];
    if local.is_empty() || domain.is_empty() {
        return REDACTED_PLACEHOLDER.to_string();
    }
    let local_prefix: String = local.chars().take(3).collect();
    // The last `.`-segment of the domain is the recognisable label
    // (`gmail.com` → `.com`, `mail.example.co.kr` → `.kr`). Keeping
    // a single segment is the smallest leak surface that still lets
    // an operator distinguish `@gmail` from `@yahoo` for triage.
    let last_segment = match domain.rfind('.') {
        Some(idx) => &domain[idx..],
        None => return REDACTED_PLACEHOLDER.to_string(),
    };
    format!("{local_prefix}***@***{last_segment}")
}

/// Trailing-4 redaction. Strings shorter than 5 chars become the
/// full placeholder — preserving "the last 4 of a 4-char value" is
/// just the original value.
pub fn redact_last4(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 5 {
        return REDACTED_PLACEHOLDER.to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("***{tail}")
}

// ---------------------------------------------------------------------------
// PiiClassifier — suggestion-only surface
// ---------------------------------------------------------------------------

/// One classifier suggestion. The ontology editor surfaces this to
/// the operator as "we noticed column X looks like Y" and asks for
/// explicit confirmation; the classifier never sets
/// [`crate::ir::PropertyDef::pii_kind`] directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PiiSuggestion {
    /// Suggested annotation. The operator may accept, override, or
    /// reject it.
    pub kind: PiiKind,
    /// 0.0–1.0 self-rated confidence. Below 0.5 generally means
    /// "ask the operator before showing the suggestion at all".
    pub confidence: f32,
    /// Human-readable explanation of *why* the classifier matched
    /// (e.g., `column name matches /\bemail\b/`). Surfaced to the
    /// operator alongside the suggestion.
    pub reason: String,
}

/// Trait for column-name / sample-value heuristics that propose
/// PII annotations. Implementations are **suggestion-only** — they
/// must never gate redaction. Returning `None` means the classifier
/// has no opinion; downstream callers fall back to "operator
/// decides".
///
/// The trait is async-free on purpose: today's heuristics are
/// regex-on-name, which fits in a synchronous fn. An LLM-based
/// classifier (a future plug-in mentioned in the plan) would either
/// pre-warm via a separate method or wrap a `tokio::task::block_in_place`.
pub trait PiiClassifier: Send + Sync {
    /// Inspect a column's name (and optional data-type hint) and
    /// optionally return a [`PiiSuggestion`].
    fn classify_column(
        &self,
        column_name: &str,
        data_type: Option<&str>,
    ) -> Option<PiiSuggestion>;
}

/// Regex-on-column-name classifier. Mainly useful at ontology
/// authoring time when the operator is annotating a brand-new
/// schema and has no other signal yet.
///
/// The patterns are deliberately conservative — `email_template_id`
/// is not flagged as `Email`, only the standalone token `email`.
/// The `\b` boundaries plus a short, reviewed list of well-known
/// names make false positives the rarer of the two failure modes.
#[derive(Debug, Default, Clone)]
pub struct RegexPiiClassifier;

impl RegexPiiClassifier {
    pub fn new() -> Self {
        Self
    }
}

impl PiiClassifier for RegexPiiClassifier {
    fn classify_column(
        &self,
        column_name: &str,
        _data_type: Option<&str>,
    ) -> Option<PiiSuggestion> {
        let lower = column_name.to_lowercase();
        for (pattern, kind, confidence, reason) in DEFAULT_PATTERNS {
            if matches_token(&lower, pattern) {
                return Some(PiiSuggestion {
                    kind: kind.clone(),
                    confidence: *confidence,
                    reason: format!("column name '{column_name}' {reason}"),
                });
            }
        }
        None
    }
}

/// `(token, kind, confidence, reason)` triples that drive the
/// regex-style classifier. `token` is a lowercase, identifier-shaped
/// substring; the matcher requires the token to appear delimited by
/// `_` / `-` / start / end so `email_template_id` does not match
/// `email`. Order matters — the first match wins, so the more
/// specific tokens come first.
#[allow(clippy::type_complexity)]
const DEFAULT_PATTERNS: &[(&str, PiiKind, f32, &str)] = &[
    ("email", PiiKind::Email, 0.9, "matches the token `email`"),
    (
        "e_mail",
        PiiKind::Email,
        0.9,
        "matches the token `e_mail`",
    ),
    ("phone", PiiKind::Phone, 0.85, "matches the token `phone`"),
    ("mobile", PiiKind::Phone, 0.7, "matches the token `mobile`"),
    ("ssn", PiiKind::Ssn, 0.95, "matches the token `ssn`"),
    (
        "credit_card",
        PiiKind::PaymentCardNumber,
        0.95,
        "matches the token `credit_card`",
    ),
    (
        "card_number",
        PiiKind::PaymentCardNumber,
        0.95,
        "matches the token `card_number`",
    ),
    (
        "card_no",
        PiiKind::PaymentCardNumber,
        0.85,
        "matches the token `card_no`",
    ),
    (
        "password",
        PiiKind::Password,
        0.95,
        "matches the token `password`",
    ),
    ("passwd", PiiKind::Password, 0.9, "matches the token `passwd`"),
    ("pwd", PiiKind::Password, 0.7, "matches the token `pwd`"),
    ("token", PiiKind::Token, 0.85, "matches the token `token`"),
    (
        "api_key",
        PiiKind::Token,
        0.85,
        "matches the token `api_key`",
    ),
    (
        "secret",
        PiiKind::Token,
        0.6,
        "matches the token `secret` (review carefully — broad match)",
    ),
];

/// Whole-token match: `token` appears in `lower` separated by `_` /
/// `-` / `.` / start / end. Plus a small foreign-key rejection
/// rule — `<token>(_or_)id` columns refer to a foreign key on
/// something *related* to the token, not the token itself
/// (`email_template_id` is a template id, not an email address).
/// The plan calls out exactly this false-positive class as the
/// reason classifier output stays suggestion-only — but
/// eliminating the easy ones at source keeps the operator's review
/// queue short.
fn matches_token(lower: &str, token: &str) -> bool {
    let bytes = lower.as_bytes();
    let tlen = token.len();
    let blen = bytes.len();
    if tlen == 0 || tlen > blen {
        return false;
    }
    let mut i = 0;
    while i + tlen <= blen {
        if &bytes[i..i + tlen] == token.as_bytes() {
            let left_ok = i == 0
                || matches!(bytes[i - 1] as char, '_' | '-' | '.');
            let right_end = i + tlen;
            let right_ok = right_end == blen
                || matches!(bytes[right_end] as char, '_' | '-' | '.');
            // Foreign-key suppression: anything ending in `_id`/`-id`
            // (after the token) is a reference to a different
            // entity, not the PII itself.
            let suffix_is_fk = right_end < blen && {
                let suffix = &lower[right_end..];
                suffix.ends_with("_id") || suffix.ends_with("-id")
            };
            if left_ok && right_ok && !suffix_is_fk {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // redact_value
    // ---------------------------------------------------------------

    #[test]
    fn email_keeps_local_prefix_and_tld() {
        assert_eq!(redact_value("john.doe@gmail.com", &PiiKind::Email), "joh***@***.com");
    }

    #[test]
    fn email_short_local_takes_what_there_is() {
        // 2-char local "ab" — keep both, append the marker. The
        // policy is "first 3 chars" with no padding.
        assert_eq!(redact_value("ab@x.io", &PiiKind::Email), "ab***@***.io");
    }

    #[test]
    fn email_with_subdomain_keeps_only_last_label() {
        assert_eq!(
            redact_value("a.b.c@mail.example.co.kr", &PiiKind::Email),
            "a.b***@***.kr"
        );
    }

    #[test]
    fn email_malformed_falls_back_to_full_redaction() {
        assert_eq!(redact_value("not-an-email", &PiiKind::Email), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("@nodomain.com", &PiiKind::Email), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("nolocal@", &PiiKind::Email), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("nodot@whatever", &PiiKind::Email), REDACTED_PLACEHOLDER);
    }

    #[test]
    fn phone_card_ssn_keep_last4() {
        assert_eq!(redact_value("1234-5678-9012-3456", &PiiKind::PaymentCardNumber), "***3456");
        assert_eq!(redact_value("123-45-6789", &PiiKind::Ssn), "***6789");
        assert_eq!(redact_value("+82-10-1234-5678", &PiiKind::Phone), "***5678");
    }

    #[test]
    fn last4_short_value_is_fully_redacted() {
        // Anything <5 chars goes to the placeholder — preserving "last
        // 4 of a 3-char value" is just the value itself.
        assert_eq!(redact_value("12", &PiiKind::Phone), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("1234", &PiiKind::Phone), REDACTED_PLACEHOLDER);
    }

    #[test]
    fn password_token_custom_use_full_placeholder() {
        assert_eq!(redact_value("hunter2", &PiiKind::Password), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("ya29.A0ARrdaM...", &PiiKind::Token), REDACTED_PLACEHOLDER);
        assert_eq!(
            redact_value("BIO-FINGERPRINT-HEX", &PiiKind::Custom("biometric_v2".into())),
            REDACTED_PLACEHOLDER
        );
    }

    #[test]
    fn redact_column_stats_redacts_samples_min_max_keeps_counts() {
        let mut stats = ColumnStats {
            column_name: "email".to_string(),
            null_count: 7,
            distinct_count: 1234,
            sample_values: vec![
                "alice@example.com".to_string(),
                "bob.smith@gmail.com".to_string(),
            ],
            min_value: Some("aaa@example.com".to_string()),
            max_value: Some("zzz@example.com".to_string()),
        };
        redact_column_stats(&mut stats, &PiiKind::Email);
        assert_eq!(stats.null_count, 7, "null_count is NOT PII");
        assert_eq!(stats.distinct_count, 1234, "distinct_count is NOT PII");
        for v in &stats.sample_values {
            assert!(v.contains("***"), "sample value not redacted: {v}");
        }
        assert!(stats.min_value.as_deref().unwrap().contains("***"));
        assert!(stats.max_value.as_deref().unwrap().contains("***"));
    }

    // ---------------------------------------------------------------
    // RegexPiiClassifier
    // ---------------------------------------------------------------

    #[test]
    fn classifier_detects_standalone_email_token() {
        let c = RegexPiiClassifier::new();
        let s = c.classify_column("email", None).expect("classify email");
        assert_eq!(s.kind, PiiKind::Email);
        assert!(s.confidence > 0.5);
    }

    #[test]
    fn classifier_does_not_falsely_match_email_template_id() {
        // The exact false positive the plan calls out — a regex
        // without word boundaries would match this. The token
        // matcher must reject it.
        let c = RegexPiiClassifier::new();
        assert!(c.classify_column("email_template_id", None).is_none());
    }

    #[test]
    fn classifier_token_is_case_insensitive() {
        let c = RegexPiiClassifier::new();
        assert!(c.classify_column("EMAIL", None).is_some());
        assert!(c.classify_column("Phone_Number", None).is_some());
    }

    #[test]
    fn classifier_returns_password_for_password_column() {
        let c = RegexPiiClassifier::new();
        let s = c.classify_column("password", None).expect("classify password");
        assert_eq!(s.kind, PiiKind::Password);
    }

    #[test]
    fn classifier_returns_token_for_api_key_column() {
        let c = RegexPiiClassifier::new();
        let s = c.classify_column("api_key", None).expect("classify api_key");
        assert_eq!(s.kind, PiiKind::Token);
    }

    #[test]
    fn classifier_returns_none_for_unrelated_column() {
        let c = RegexPiiClassifier::new();
        assert!(c.classify_column("created_at", None).is_none());
        assert!(c.classify_column("user_id", None).is_none());
    }

    #[test]
    fn matches_token_word_boundaries_pin_known_cases() {
        // Whole-name and well-delimited subtokens match.
        assert!(matches_token("email", "email"));
        assert!(matches_token("user_email", "email"));
        assert!(matches_token("email_addr", "email"));
        assert!(matches_token("primary-email-1", "email"));
        // No-delimiter runs do NOT match.
        assert!(!matches_token("emailtemplateid", "email"));
        // Foreign-key suppression: the entire false-positive class
        // the plan calls out goes here.
        assert!(!matches_token("email_template_id", "email"));
        assert!(!matches_token("email_id", "email"));
        assert!(!matches_token("phone_number_id", "phone"));
    }
}

//! PII annotation + redaction + classification.
//!
//! The PII surface has three layers, each owned by this module:
//!
//! 1. **Annotation** — a user decision recorded on
//!    [`crate::ir::PropertyDef::pii_kind`] or carried into ontology
//!    design via [`PiiAnnotation`]. The annotation is the single
//!    source of truth at runtime.
//! 2. **Classification** — a [`PiiClassifier`] inspects column
//!    metadata + sample values and emits a [`PiiSuggestion`]. The
//!    suggestion is **advisory only**; the operator confirms before
//!    the kind is committed to an annotation.
//! 3. **Redaction** — [`redact_value`] / [`redact_column_stats`]
//!    rewrite a stored value according to its annotated kind.
//!    Deterministic and shape-preserving where the kind benefits
//!    (email keeps a 3-char local prefix + TLD; numeric-tail kinds
//!    keep the trailing 4 chars; secrets become `<redacted>`).
//!
//! The split is intentional: classification carries inherent
//! false-positive / false-negative risk
//! (`email_template_id` matching `email`, `e_addr` missing
//! `email`). Letting it drive redaction would smuggle the same
//! errors into the redacted output. Annotation is the gate
//! between the two.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ox_core::source_schema::ColumnStats;

use crate::ir::{DataClassification, PiiKind};

// ---------------------------------------------------------------------------
// Annotation
// ---------------------------------------------------------------------------

/// User-confirmed PII annotation for a source column. Carries the
/// classifier's role from suggestion to commitment: at design
/// time, every entry here is treated as authoritative for both the
/// resulting [`crate::ir::PropertyDef::pii_kind`] and any sample-
/// value redaction performed before the LLM sees the column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
pub struct PiiAnnotation {
    pub table: String,
    pub column: String,
    pub kind: PiiKind,
}

/// Source column the operator wants withheld from ontology design
/// entirely. The column's metadata and sample values never reach
/// the LLM, and the resulting ontology contains no
/// [`crate::ir::PropertyDef`] for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
pub struct ExcludedColumn {
    pub table: String,
    pub column: String,
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// Inputs available to a [`PiiClassifier`]. Every signal is
/// optional — a classifier only consults what it needs.
#[derive(Debug, Clone)]
pub struct PiiSignals<'a> {
    pub table: &'a str,
    pub column: &'a str,
    pub data_type: Option<&'a str>,
    pub sample_values: &'a [String],
}

/// Classifier output. Returned from [`PiiClassifier::classify`] when
/// the inputs raise the classifier's confidence above the
/// emit-threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
pub struct PiiSuggestion {
    pub table: String,
    pub column: String,
    pub kind: PiiKind,
    /// 0.0–1.0 self-rated confidence.
    pub confidence: f32,
    pub reason: String,
}

/// Inspect a column and optionally suggest a [`PiiKind`].
/// Implementations are **advisory only** — they must never gate
/// redaction. `None` means the classifier has no opinion;
/// downstream callers fall back to "operator decides".
pub trait PiiClassifier: Send + Sync {
    fn classify(&self, signals: &PiiSignals<'_>) -> Option<PiiSuggestion>;
}

/// Combine multiple classifiers. The first classifier whose
/// [`PiiClassifier::classify`] returns `Some` wins — register the
/// most specific classifier first (LLM-based, then regex-based,
/// then sample-pattern-based) so a high-confidence call short-
/// circuits the rest.
#[derive(Default)]
pub struct CompositePiiClassifier {
    inner: Vec<Arc<dyn PiiClassifier>>,
}

impl CompositePiiClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, classifier: Arc<dyn PiiClassifier>) -> Self {
        self.inner.push(classifier);
        self
    }
}

impl PiiClassifier for CompositePiiClassifier {
    fn classify(&self, signals: &PiiSignals<'_>) -> Option<PiiSuggestion> {
        self.inner.iter().find_map(|c| c.classify(signals))
    }
}

/// Regex-style classifier driven by token-bounded column-name
/// matching plus a sample-value email-shape probe.
///
/// Patterns are deliberately conservative — `email_template_id` is
/// not flagged as `Email`, only the standalone token `email` (or
/// `email` followed by a non-`_id` suffix). The whole-token
/// requirement plus a small reviewed list keeps false positives the
/// rarer of the two failure modes.
#[derive(Debug, Default, Clone)]
pub struct RegexPiiClassifier;

impl RegexPiiClassifier {
    pub fn new() -> Self {
        Self
    }
}

impl PiiClassifier for RegexPiiClassifier {
    fn classify(&self, signals: &PiiSignals<'_>) -> Option<PiiSuggestion> {
        let lower = signals.column.to_lowercase();
        for entry in DEFAULT_PATTERNS {
            if matches_token(&lower, entry.token) {
                return Some(PiiSuggestion {
                    table: signals.table.to_string(),
                    column: signals.column.to_string(),
                    kind: entry.kind.clone(),
                    confidence: entry.confidence,
                    reason: format!(
                        "column name '{}' matches the token `{}`",
                        signals.column, entry.token
                    ),
                });
            }
        }
        // Sample-value fallback: an unflagged column whose values
        // shape like emails still gets a low-confidence suggestion
        // so the operator sees it for review.
        if looks_like_email_samples(signals.sample_values) {
            return Some(PiiSuggestion {
                table: signals.table.to_string(),
                column: signals.column.to_string(),
                kind: PiiKind::Email,
                confidence: 0.6,
                reason: format!(
                    "column '{}' sample values contain `@` and `.` in an email-shaped form",
                    signals.column
                ),
            });
        }
        None
    }
}

struct PatternEntry {
    token: &'static str,
    kind: PiiKind,
    confidence: f32,
}

const DEFAULT_PATTERNS: &[PatternEntry] = &[
    // --- Identity ---
    PatternEntry { token: "email",       kind: PiiKind::Email,             confidence: 0.9 },
    PatternEntry { token: "e_mail",      kind: PiiKind::Email,             confidence: 0.9 },
    PatternEntry { token: "phone",       kind: PiiKind::Phone,             confidence: 0.85 },
    PatternEntry { token: "mobile",      kind: PiiKind::Phone,             confidence: 0.7 },
    PatternEntry { token: "tel",         kind: PiiKind::Phone,             confidence: 0.65 },
    PatternEntry { token: "name",        kind: PiiKind::Name,              confidence: 0.6 },
    PatternEntry { token: "birth",       kind: PiiKind::DateOfBirth,       confidence: 0.7 },
    PatternEntry { token: "dob",         kind: PiiKind::DateOfBirth,       confidence: 0.85 },
    PatternEntry { token: "ssn",         kind: PiiKind::Ssn,               confidence: 0.95 },
    PatternEntry { token: "rrn",         kind: PiiKind::Ssn,               confidence: 0.9 },
    PatternEntry { token: "nin",         kind: PiiKind::Ssn,               confidence: 0.85 },
    PatternEntry { token: "passport",    kind: PiiKind::Passport,          confidence: 0.9 },
    PatternEntry { token: "license",     kind: PiiKind::DriversLicense,    confidence: 0.7 },
    // --- Address ---
    PatternEntry { token: "address",     kind: PiiKind::Address,           confidence: 0.7 },
    PatternEntry { token: "addr",        kind: PiiKind::Address,           confidence: 0.7 },
    PatternEntry { token: "street",      kind: PiiKind::Address,           confidence: 0.7 },
    PatternEntry { token: "zip",         kind: PiiKind::Address,           confidence: 0.6 },
    PatternEntry { token: "postal",      kind: PiiKind::Address,           confidence: 0.6 },
    // --- Network / Geo ---
    PatternEntry { token: "ipaddr",      kind: PiiKind::IpAddress,         confidence: 0.85 },
    PatternEntry { token: "ipv4",        kind: PiiKind::IpAddress,         confidence: 0.9 },
    PatternEntry { token: "ipv6",        kind: PiiKind::IpAddress,         confidence: 0.9 },
    PatternEntry { token: "latitude",    kind: PiiKind::GeoLocation,       confidence: 0.85 },
    PatternEntry { token: "longitude",   kind: PiiKind::GeoLocation,       confidence: 0.85 },
    PatternEntry { token: "geolocation", kind: PiiKind::GeoLocation,       confidence: 0.9 },
    PatternEntry { token: "geohash",     kind: PiiKind::GeoLocation,       confidence: 0.85 },
    // --- Financial ---
    PatternEntry { token: "credit_card", kind: PiiKind::PaymentCardNumber, confidence: 0.95 },
    PatternEntry { token: "creditcard",  kind: PiiKind::PaymentCardNumber, confidence: 0.95 },
    PatternEntry { token: "card_number", kind: PiiKind::PaymentCardNumber, confidence: 0.95 },
    PatternEntry { token: "cardnumber",  kind: PiiKind::PaymentCardNumber, confidence: 0.95 },
    PatternEntry { token: "card_no",     kind: PiiKind::PaymentCardNumber, confidence: 0.85 },
    PatternEntry { token: "ccnumber",    kind: PiiKind::PaymentCardNumber, confidence: 0.9 },
    PatternEntry { token: "pan",         kind: PiiKind::PaymentCardNumber, confidence: 0.6 },
    PatternEntry { token: "iban",        kind: PiiKind::Iban,              confidence: 0.95 },
    PatternEntry { token: "bankaccount", kind: PiiKind::BankAccountNumber, confidence: 0.9 },
    PatternEntry { token: "routing",     kind: PiiKind::BankAccountNumber, confidence: 0.6 },
    // --- Health ---
    PatternEntry { token: "mrn",         kind: PiiKind::MedicalRecordNumber, confidence: 0.9 },
    PatternEntry { token: "medicalrecord", kind: PiiKind::MedicalRecordNumber, confidence: 0.95 },
    PatternEntry { token: "insurance",   kind: PiiKind::InsuranceId,       confidence: 0.7 },
    // --- Biometric ---
    PatternEntry { token: "fingerprint", kind: PiiKind::Biometric,         confidence: 0.95 },
    PatternEntry { token: "biometric",   kind: PiiKind::Biometric,         confidence: 0.95 },
    PatternEntry { token: "faceid",      kind: PiiKind::Biometric,         confidence: 0.9 },
    PatternEntry { token: "voiceprint",  kind: PiiKind::Biometric,         confidence: 0.9 },
    // --- Auth secrets ---
    PatternEntry { token: "password",    kind: PiiKind::Password,          confidence: 0.95 },
    PatternEntry { token: "passwd",      kind: PiiKind::Password,          confidence: 0.9 },
    PatternEntry { token: "pwd",         kind: PiiKind::Password,          confidence: 0.7 },
    PatternEntry { token: "token",       kind: PiiKind::Token,             confidence: 0.85 },
    PatternEntry { token: "api_key",     kind: PiiKind::Token,             confidence: 0.85 },
    PatternEntry { token: "apikey",      kind: PiiKind::Token,             confidence: 0.85 },
    PatternEntry { token: "secret",      kind: PiiKind::Token,             confidence: 0.6 },
];

fn looks_like_email_samples(samples: &[String]) -> bool {
    samples
        .iter()
        .any(|v| v.len() > 5 && v.contains('@') && v.contains('.'))
}

/// Token match — `token` appears in `lower` separated by `_` / `-`
/// / `.` / start / end. Plus a foreign-key suppression that
/// rejects `<token>(_|-)id` shapes (`email_template_id` is a
/// template id, not an email address).
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

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// Placeholder substituted for a fully-redacted value (Password,
/// Token, Custom, and any other kind without a partial-redaction
/// rule). Pinned as a `pub const` so wire-shape clients can
/// case-fold against the same string.
pub const REDACTED_PLACEHOLDER: &str = "<redacted>";

/// Redact a single value according to its `kind`.
pub fn redact_value(value: &str, kind: &PiiKind) -> String {
    match kind {
        PiiKind::Email => redact_email(value),
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

/// Redact every sample value, min, and max in `stats` according to
/// `kind`. Counts (`null_count`, `distinct_count`) are not
/// redacted — counts are not PII and operators rely on the
/// distribution to detect schema drift.
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

/// Email redaction: first 3 chars of the local part + `***`, then
/// `***` + last domain segment (TLD). Inputs that fail to parse as
/// an email-shaped string fall back to the full-replacement
/// placeholder — over-redact rather than leak a partial identifier
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
    let last_segment = match domain.rfind('.') {
        Some(idx) => &domain[idx..],
        None => return REDACTED_PLACEHOLDER.to_string(),
    };
    format!("{local_prefix}***@***{last_segment}")
}

/// Trailing-4 redaction. Strings shorter than 5 chars become the
/// full placeholder — keeping "the last 4 of a 4-char value" would
/// just be the original value.
pub fn redact_last4(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 5 {
        return REDACTED_PLACEHOLDER.to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("***{tail}")
}

// ---------------------------------------------------------------------------
// Classification mapping
// ---------------------------------------------------------------------------

/// Map a [`PiiKind`] onto its [`DataClassification`]. Drives the
/// `classification` field on [`crate::ir::PropertyDef`] so
/// downstream policies (audit, retention, role-based access) can
/// react with one enum check.
pub fn data_classification_for(kind: &PiiKind) -> DataClassification {
    match kind {
        // Generic personal data — sensitive but not regulator-imposed.
        PiiKind::Email
        | PiiKind::Phone
        | PiiKind::Name
        | PiiKind::Address
        | PiiKind::IpAddress => DataClassification::Confidential,
        // Regulator-imposed: govt ID, finance, health, biometric, precise geo.
        PiiKind::NationalId { .. }
        | PiiKind::Ssn
        | PiiKind::Passport
        | PiiKind::DriversLicense
        | PiiKind::DateOfBirth
        | PiiKind::PaymentCardNumber
        | PiiKind::CreditCard
        | PiiKind::BankAccountNumber
        | PiiKind::Iban
        | PiiKind::MedicalRecordNumber
        | PiiKind::InsuranceId
        | PiiKind::Biometric
        | PiiKind::GeoLocation => DataClassification::Restricted,
        // Auth secrets — Restricted-tier; never expose, never log.
        PiiKind::Password | PiiKind::Token => DataClassification::Restricted,
        // Reviewer-defined — without further info, treat as Internal.
        PiiKind::Custom(_) => DataClassification::Internal,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn signals<'a>(table: &'a str, column: &'a str, samples: &'a [String]) -> PiiSignals<'a> {
        PiiSignals {
            table,
            column,
            data_type: None,
            sample_values: samples,
        }
    }

    // ---------------- redact_value ----------------

    #[test]
    fn email_keeps_local_prefix_and_tld() {
        assert_eq!(redact_value("john.doe@gmail.com", &PiiKind::Email), "joh***@***.com");
    }

    #[test]
    fn email_short_local_takes_what_there_is() {
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
        assert_eq!(redact_value("12", &PiiKind::Phone), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("1234", &PiiKind::Phone), REDACTED_PLACEHOLDER);
    }

    #[test]
    fn password_token_custom_use_full_placeholder() {
        assert_eq!(redact_value("hunter2", &PiiKind::Password), REDACTED_PLACEHOLDER);
        assert_eq!(redact_value("ya29.A0ARrdaM", &PiiKind::Token), REDACTED_PLACEHOLDER);
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
            pii_redacted: None,
        };
        redact_column_stats(&mut stats, &PiiKind::Email);
        assert_eq!(stats.null_count, 7);
        assert_eq!(stats.distinct_count, 1234);
        for v in &stats.sample_values {
            assert!(v.contains("***"), "sample value not redacted: {v}");
        }
        assert!(stats.min_value.as_deref().unwrap().contains("***"));
        assert!(stats.max_value.as_deref().unwrap().contains("***"));
    }

    // ---------------- RegexPiiClassifier ----------------

    #[test]
    fn classifier_detects_standalone_email_token() {
        let c = RegexPiiClassifier::new();
        let s = c.classify(&signals("users", "email", &[])).unwrap();
        assert_eq!(s.kind, PiiKind::Email);
        assert!(s.confidence > 0.5);
    }

    #[test]
    fn classifier_does_not_falsely_match_email_template_id() {
        let c = RegexPiiClassifier::new();
        assert!(c.classify(&signals("t", "email_template_id", &[])).is_none());
    }

    #[test]
    fn classifier_token_is_case_insensitive() {
        let c = RegexPiiClassifier::new();
        assert!(c.classify(&signals("t", "EMAIL", &[])).is_some());
        assert!(c.classify(&signals("t", "Phone_Number", &[])).is_some());
    }

    #[test]
    fn classifier_returns_password_for_password_column() {
        let c = RegexPiiClassifier::new();
        let s = c.classify(&signals("u", "password", &[])).unwrap();
        assert_eq!(s.kind, PiiKind::Password);
    }

    #[test]
    fn classifier_returns_token_for_api_key_column() {
        let c = RegexPiiClassifier::new();
        let s = c.classify(&signals("u", "api_key", &[])).unwrap();
        assert_eq!(s.kind, PiiKind::Token);
    }

    #[test]
    fn classifier_falls_back_to_email_pattern_in_samples() {
        let c = RegexPiiClassifier::new();
        let samples = vec!["user@example.com".to_string()];
        let s = c
            .classify(&signals("t", "contact", &samples))
            .expect("classify by sample shape");
        assert_eq!(s.kind, PiiKind::Email);
        assert!(s.confidence < 0.7);
    }

    #[test]
    fn classifier_returns_none_for_unrelated_column() {
        let c = RegexPiiClassifier::new();
        assert!(c.classify(&signals("t", "created_at", &[])).is_none());
        assert!(c.classify(&signals("t", "user_id", &[])).is_none());
    }

    #[test]
    fn matches_token_word_boundaries_pin_known_cases() {
        assert!(matches_token("email", "email"));
        assert!(matches_token("user_email", "email"));
        assert!(matches_token("email_addr", "email"));
        assert!(matches_token("primary-email-1", "email"));
        assert!(!matches_token("emailtemplateid", "email"));
        assert!(!matches_token("email_template_id", "email"));
        assert!(!matches_token("email_id", "email"));
        assert!(!matches_token("phone_number_id", "phone"));
    }

    // ---------------- CompositePiiClassifier ----------------

    #[test]
    fn composite_returns_first_match() {
        struct Always(PiiKind);
        impl PiiClassifier for Always {
            fn classify(&self, s: &PiiSignals<'_>) -> Option<PiiSuggestion> {
                Some(PiiSuggestion {
                    table: s.table.to_string(),
                    column: s.column.to_string(),
                    kind: self.0.clone(),
                    confidence: 1.0,
                    reason: "always".into(),
                })
            }
        }
        struct Never;
        impl PiiClassifier for Never {
            fn classify(&self, _: &PiiSignals<'_>) -> Option<PiiSuggestion> {
                None
            }
        }
        let composite = CompositePiiClassifier::new()
            .with(Arc::new(Always(PiiKind::Email)))
            .with(Arc::new(Always(PiiKind::Phone)));
        let s = composite.classify(&signals("t", "x", &[])).unwrap();
        assert_eq!(s.kind, PiiKind::Email);

        let composite = CompositePiiClassifier::new()
            .with(Arc::new(Never))
            .with(Arc::new(Always(PiiKind::Phone)));
        let s = composite.classify(&signals("t", "x", &[])).unwrap();
        assert_eq!(s.kind, PiiKind::Phone);
    }

    // ---------------- data_classification_for ----------------

    #[test]
    fn classification_map_groups_kinds_correctly() {
        assert_eq!(data_classification_for(&PiiKind::Email), DataClassification::Confidential);
        assert_eq!(
            data_classification_for(&PiiKind::Ssn),
            DataClassification::Restricted
        );
        assert_eq!(
            data_classification_for(&PiiKind::Password),
            DataClassification::Restricted
        );
        assert_eq!(
            data_classification_for(&PiiKind::Custom("misc".into())),
            DataClassification::Internal
        );
    }
}

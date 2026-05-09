//! Structured diagnostic message — RFC 7807 / gRPC `google.rpc.Status`
//! pattern for system-emitted errors and warnings.
//!
//! [`DiagnosticMessage`] is the canonical wire shape for *anything*
//! the platform reports back to the user that is not authored
//! domain content (rule names, type descriptions, glossary entries
//! — those stay [`crate::LocalizedText`]).
//!
//! ## Why not [`crate::LocalizedText`]?
//!
//! `LocalizedText` is for human-authored text whose translations are
//! *content*, written and stored alongside the data. Adding a new
//! UI language requires editing every `LocalizedText` row.
//!
//! `DiagnosticMessage` is for machine-authored text whose translations
//! are *catalogue entries*, resolved client-side via i18n
//! (`next-intl`, ICU MessageFormat). Adding a new UI language
//! requires adding one catalogue file — call sites never change.
//!
//! The split mirrors how every mature platform models the same
//! problem: Stripe API errors carry `code` + `message`; GitHub
//! REST returns `message` + `errors[].code`; AWS APIs return
//! `Error.Code` + `Error.Message`; Kubernetes admission webhooks
//! emit `Status.reason` + `Status.message`. The English `message`
//! is the universal human-readable fallback for logs, alerting,
//! and operators whose locale is not yet catalogued; `code` +
//! `params` is the structured handle the FE renders through ICU.
//!
//! ## Wire shape
//!
//! ```json
//! {
//!   "code": "ontology.validate.duplicate_property_id",
//!   "message": "Node 'Customer' has duplicate property id 'email'",
//!   "params": { "owner_kind": "Node", "owner_label": "Customer", "id": "email" }
//! }
//! ```
//!
//! - `code` — stable dotted identifier (`<surface>.<phase>.<kind>`),
//!   never user-facing, safe to grep, safe to evolve catalogue keys
//!   against. Treat the catalogue key set as a public API.
//! - `message` — English, machine-rendered. Goes to logs, alerts,
//!   and any consumer without a catalogue entry.
//! - `params` — placeholder values for the catalogue template
//!   (ICU MessageFormat). Order-independent; key names match the
//!   ICU placeholders. Empty map serializes away.
//!
//! ## Construction
//!
//! ```ignore
//! use ox_core::diagnostic::diag;
//!
//! let d = diag("ontology.validate.duplicate_property_id")
//!     .with("owner_kind", "Node")
//!     .with("owner_label", "Customer")
//!     .with("id", "email")
//!     .message(format!(
//!         "Node '{}' has duplicate property id '{}'",
//!         "Customer", "email"
//!     ));
//! ```

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Structured diagnostic — `code` + English `message` + `params`.
///
/// See module docs for design rationale and wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct DiagnosticMessage {
    /// Stable dotted identifier (`<surface>.<phase>.<kind>`). Treat
    /// the catalogue key set as a public API — renaming a code is
    /// a breaking change for any consumer that has localised it.
    pub code: String,

    /// English human-readable rendering. Always present; serves as
    /// the universal fallback for logs, alerts, and consumers
    /// without a catalogue entry for `code`.
    pub message: String,

    /// Placeholder values for the catalogue template
    /// (ICU MessageFormat). Keys match the ICU placeholders in the
    /// localised template. `BTreeMap` for stable serialisation
    /// order (deterministic logs, deterministic snapshot tests).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(value_type = std::collections::BTreeMap<String, serde_json::Value>)]
    pub params: BTreeMap<String, serde_json::Value>,
}

impl DiagnosticMessage {
    /// Open a builder for the given code. Prefer the free
    /// [`diag`] helper at call sites for terseness.
    ///
    /// `code` must follow the canonical
    /// `<crate>.<surface>.<kind>` shape — at least two dotted
    /// segments, each `[a-z][a-z0-9_]*`. Violations panic in debug
    /// builds via [`debug_assert!`] so test runs catch malformed
    /// codes; release builds skip the check (zero cost on the hot
    /// path).
    pub fn builder(code: impl Into<String>) -> DiagnosticBuilder {
        let code = code.into();
        debug_assert!(
            is_valid_diagnostic_code(&code),
            "diagnostic code `{code}` does not follow the canonical \
             `<crate>.<surface>.<kind>` shape (≥2 dotted segments, \
             each `[a-z][a-z0-9_]*`)"
        );
        DiagnosticBuilder {
            code,
            params: BTreeMap::new(),
        }
    }
}

/// Canonical diagnostic-code format check:
///
/// - At least two dotted segments (`a.b`, `a.b.c`, …).
/// - Each segment starts with an ASCII lowercase letter.
/// - Subsequent characters are lowercase ASCII letters, digits, or
///   underscores.
///
/// Returns `false` for the empty string, single-segment codes, or
/// any segment containing uppercase / hyphens / spaces. Used by the
/// [`debug_assert!`] in [`DiagnosticMessage::builder`] to enforce the
/// convention without paying a regex dependency on the workspace's
/// bottom-of-the-graph crate.
pub fn is_valid_diagnostic_code(code: &str) -> bool {
    let mut segments = code.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    if !is_valid_segment(first) {
        return false;
    }
    let mut rest_count = 0usize;
    for seg in segments {
        rest_count += 1;
        if !is_valid_segment(seg) {
            return false;
        }
    }
    rest_count >= 1
}

fn is_valid_segment(seg: &str) -> bool {
    let mut chars = seg.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Display = the English `message`. Operator logs, alerts, and any
/// `format!("{}", diag)` call site land on the universal English
/// rendering — matching the convention every Sentry / Datadog
/// log line follows for structured errors.
impl fmt::Display for DiagnosticMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Builder for [`DiagnosticMessage`]. Terminal call is
/// [`DiagnosticBuilder::message`], which consumes the builder and
/// returns the finished diagnostic.
#[derive(Debug, Clone)]
pub struct DiagnosticBuilder {
    code: String,
    params: BTreeMap<String, serde_json::Value>,
}

impl DiagnosticBuilder {
    /// Attach a `name=value` parameter. Anything implementing
    /// [`Into<serde_json::Value>`] works — strings, integers,
    /// booleans, arrays — so call sites stay free of explicit
    /// conversions.
    pub fn with(mut self, name: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.params.insert(name.into(), value.into());
        self
    }

    /// Finalise with the English message rendering.
    pub fn message(self, message: impl Into<String>) -> DiagnosticMessage {
        DiagnosticMessage {
            code: self.code,
            message: message.into(),
            params: self.params,
        }
    }
}

/// Free-standing builder shorthand. `diag(code)` is the canonical
/// emit-site form — terse enough to stay out of the way and
/// uniform enough to grep across the workspace.
pub fn diag(code: impl Into<String>) -> DiagnosticBuilder {
    DiagnosticMessage::builder(code)
}

/// Join the English `message` rendering of every diagnostic with
/// `separator`. Used at consumer boundaries that emit a single
/// flat string (e.g. `OxError::Compilation { message }`,
/// `AppError::unprocessable(...)`) — those callers want one
/// human-readable line, not a structured array. The structured
/// channel (FE catalogue, signal store) reads `code` + `params`
/// directly and never goes through this helper.
pub fn join_messages(diagnostics: &[DiagnosticMessage], separator: &str) -> String {
    diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join(separator)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builder_roundtrip() {
        let d = diag("ontology.validate.duplicate_property_id")
            .with("owner_kind", "Node")
            .with("owner_label", "Customer")
            .with("id", "email")
            .message("Node 'Customer' has duplicate property id 'email'");
        assert_eq!(d.code, "ontology.validate.duplicate_property_id");
        assert_eq!(
            d.message,
            "Node 'Customer' has duplicate property id 'email'"
        );
        assert_eq!(d.params.get("owner_kind"), Some(&json!("Node")));
        assert_eq!(d.params.get("owner_label"), Some(&json!("Customer")));
        assert_eq!(d.params.get("id"), Some(&json!("email")));
    }

    #[test]
    fn serializes_canonical_shape() {
        let d = diag("ontology.validate.duplicate_property_id")
            .with("owner_kind", "Node")
            .with("owner_label", "Customer")
            .with("id", "email")
            .message("Node 'Customer' has duplicate property id 'email'");
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(
            v,
            json!({
                "code": "ontology.validate.duplicate_property_id",
                "message": "Node 'Customer' has duplicate property id 'email'",
                "params": {
                    "id": "email",
                    "owner_kind": "Node",
                    "owner_label": "Customer"
                }
            })
        );
    }

    #[test]
    fn empty_params_omitted() {
        let d = diag("runtime.cypher.empty_pattern").message("Empty pattern in MATCH clause");
        let s = serde_json::to_string(&d).unwrap();
        assert!(
            !s.contains("params"),
            "empty params must be omitted, got {s}"
        );
    }

    #[test]
    fn deserialises_with_or_without_params() {
        let with_params: DiagnosticMessage =
            serde_json::from_str(r#"{"code":"x.y.z","message":"hi","params":{"a":1,"b":"two"}}"#)
                .unwrap();
        assert_eq!(with_params.params.get("a"), Some(&json!(1)));
        assert_eq!(with_params.params.get("b"), Some(&json!("two")));

        let without_params: DiagnosticMessage =
            serde_json::from_str(r#"{"code":"x.y.z","message":"hi"}"#).unwrap();
        assert!(without_params.params.is_empty());
    }

    #[test]
    fn params_preserve_value_kinds() {
        let d = diag("runtime.cost.budget_exceeded")
            .with("budget_ms", 500)
            .with("estimated_ms", 1234)
            .with("kinds", json!(["nodes", "edges"]))
            .with("dry_run", true)
            .message("Budget exceeded");
        assert_eq!(d.params.get("budget_ms"), Some(&json!(500)));
        assert_eq!(d.params.get("estimated_ms"), Some(&json!(1234)));
        assert_eq!(d.params.get("kinds"), Some(&json!(["nodes", "edges"])));
        assert_eq!(d.params.get("dry_run"), Some(&json!(true)));
    }

    #[test]
    fn params_serialisation_is_key_sorted() {
        let d = diag("test.params_sorted")
            .with("zeta", 1)
            .with("alpha", 2)
            .with("middle", 3)
            .message("m");
        let s = serde_json::to_string(&d).unwrap();
        let alpha = s.find("alpha").unwrap();
        let middle = s.find("middle").unwrap();
        let zeta = s.find("zeta").unwrap();
        assert!(
            alpha < middle && middle < zeta,
            "BTreeMap must serialise keys sorted: {s}"
        );
    }

    // -- Diagnostic code format invariant --------------------------------------

    #[test]
    fn valid_codes_are_accepted() {
        for code in [
            "a.b",
            "a.b.c",
            "ontology.validate.property.duplicate_id",
            "runtime.cypher.shacl.min_count_missing",
            "x.y123.z_under_score",
        ] {
            assert!(
                is_valid_diagnostic_code(code),
                "code `{code}` should be accepted"
            );
        }
    }

    #[test]
    fn invalid_codes_are_rejected() {
        for code in [
            "",       // empty
            "x",      // single segment
            ".x",     // leading dot
            "x.",     // trailing dot
            "x..y",   // empty middle segment
            "X.y",    // uppercase head
            "x.Y",    // uppercase second segment
            "1x.y",   // digit head
            "x-y.z",  // hyphen
            "x.y-z",  // hyphen in second segment
            "x y.z",  // space
            "x.y.z!", // punctuation
        ] {
            assert!(
                !is_valid_diagnostic_code(code),
                "code `{code}` should be rejected"
            );
        }
    }

    #[test]
    #[should_panic(expected = "does not follow the canonical")]
    fn debug_assert_fires_on_malformed_code() {
        let _ = diag("MalFormed").message("nope");
    }
}

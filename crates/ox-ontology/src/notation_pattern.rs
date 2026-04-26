//! Notation patterns — structured identifier format declarations.
//!
//! A [`NotationPatternDef`] constrains the shape of a property's
//! string value to a well-defined format composed of named
//! components. The canonical example is a campaign code like
//! `"SPRING_26_001"` — decomposable into
//! `{campaign: "SPRING", year: 26, seq: 1}`.
//!
//! The pattern is bidirectional: the runtime can
//! - **parse** a value into its components (`"SPRING_26_001"` →
//!   `{campaign: "SPRING", year: 26, seq: 1}`),
//! - **render** components back into a value
//!   (`{campaign: "FALL", year: 26, seq: 42}` → `"FALL_26_042"`),
//! - **validate** a candidate string against the pattern without
//!   materialising components.
//!
//! ## Conceptual reference
//!
//! - **HL7 FHIR NamingSystem** — declares identifier-system format
//!   rules (DEA number, NPI, HL7 v2 identifier).
//! - **ISO/IEC 11179 Part 5** — naming and identification
//!   principles; composable identifiers with named parts.
//! - **schema.org PropertyValueSpecification.pattern** — regex-style
//!   constraint on a property value.
//!
//! Notation patterns complement [`crate::value_set::ValueSetDef`]:
//! value sets constrain a property to one of N enumerated codes,
//! notation patterns constrain a property to a structured format
//! whose components may themselves be value-set-constrained (the
//! `campaign` component of a campaign code is a code from the
//! `Seasons` value set).
//!
//! ## Why not just a regex?
//!
//! A regex validates but does not structure. With a
//! [`NotationPatternDef`], the server decomposes a value into typed
//! components that LLM prompts, admin UIs, and downstream
//! transformations can reason about: "show me all SPRING campaigns
//! regardless of year" is a component-level query. A regex would
//! force each consumer to re-parse ad-hoc.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::value_set::ValueSetId;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`NotationPatternDef`].
    NotationPatternId
);

/// A structured identifier pattern. Parse / render / validate
/// semantics are all driven by the ordered [`components`] list —
/// the [`template`] string is documentation only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct NotationPatternDef {
    pub id: NotationPatternId,

    /// Short human identifier (`"CampaignCode"`).
    pub name: String,

    #[serde(default)]
    pub display_name: LocalizedText,

    #[serde(default)]
    pub description: LocalizedText,

    /// Human-readable template for docs and UI help.
    /// **Not** used by the parser — components + separator drive
    /// the actual semantics. Example: `"{campaign}_{year:%02d}_{seq:%03d}"`.
    pub template: String,

    /// Separator string between components. Empty string means
    /// components are concatenated back-to-back (for formats like
    /// Korean national ID: `YYMMDD-NNNNNNN` has `-` separator,
    /// `YYMMDDNNNNNNN` has empty).
    #[serde(default)]
    pub separator: String,

    /// Ordered component list. Parse operates left-to-right.
    pub components: Vec<NotationComponent>,

    /// Curated valid examples for documentation + property UI
    /// placeholder text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
}

/// One component of a notation pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct NotationComponent {
    /// Component name used as the key in parse output
    /// (`{campaign: "SPRING"}`).
    pub name: String,

    #[serde(default)]
    pub display: LocalizedText,

    pub kind: NotationComponentKind,
}

/// Kind of value a [`NotationComponent`] carries. Each variant is
/// self-validating at parse time — `IntegerRange` rejects values
/// outside `[min, max]`, `Alphanumeric` rejects wrong-length
/// tokens, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotationComponentKind {
    /// Component must be a code from the referenced value set.
    /// Parser looks up the token in the expanded set; rendering
    /// emits the code verbatim.
    CodeFromSet { value_set_id: ValueSetId },
    /// Zero-padded non-negative integer in `[min, max]` with fixed
    /// decimal width. Example: `year: 26` with `width=2` →
    /// `"26"`; `seq: 1` with `width=3` → `"001"`.
    IntegerRange {
        min: i64,
        max: i64,
        /// Exact decimal digit count. `0` disables padding (variable
        /// width); non-zero enforces exact match.
        width: u8,
    },
    /// Fixed-width alphanumeric token.
    Alphanumeric { width: u32, uppercase: bool },
    /// Free text up to `max_len` characters. Use sparingly — a
    /// free-text component in the middle of a pattern makes
    /// parsing ambiguous. Prefer placing it last.
    FreeText { max_len: Option<u32> },
}

// ---------------------------------------------------------------------------
// Parse / render / validate — operational helpers
// ---------------------------------------------------------------------------

/// One extracted component from a parsed notation value. The
/// `raw` string is the substring as it appeared in the input; the
/// `typed` form is the semantic interpretation (integer, code id,
/// free text). A consumer can use either depending on whether it
/// needs the original form or the typed one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedComponent {
    pub name: String,
    pub raw: String,
    pub typed: ParsedValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedValue {
    Code(String),
    Integer(i64),
    Alphanumeric(String),
    Text(String),
}

/// Why a value did not parse. A single descriptive enum rather
/// than a flat string so the admin UI can surface the exact
/// failure (which component, what the input looked like).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotationError {
    #[error("expected {expected} components separated by '{separator}', got {found}")]
    ComponentCount {
        expected: usize,
        found: usize,
        separator: String,
    },
    #[error(
        "component '{component}' must be one of the allowed codes; got '{got}'"
    )]
    UnknownCode { component: String, got: String },
    #[error(
        "component '{component}' must be an integer in [{min}, {max}]; got '{got}'"
    )]
    IntegerOutOfRange {
        component: String,
        min: i64,
        max: i64,
        got: String,
    },
    #[error(
        "component '{component}' must be {width} digits wide; got '{got}'"
    )]
    IntegerWidthMismatch {
        component: String,
        width: u8,
        got: String,
    },
    #[error(
        "component '{component}' must be {width} alphanumeric characters; got '{got}'"
    )]
    AlphanumericWidthMismatch {
        component: String,
        width: u32,
        got: String,
    },
    #[error(
        "component '{component}' must be alphanumeric; got '{got}'"
    )]
    AlphanumericInvalid { component: String, got: String },
    #[error(
        "component '{component}' exceeds max_len {max}; got length {got}"
    )]
    FreeTextTooLong {
        component: String,
        max: u32,
        got: usize,
    },
    #[error(
        "component '{component}' expected {width} digits but '{got}' contains non-digit characters"
    )]
    IntegerNonDigit {
        component: String,
        width: u8,
        got: String,
    },
}

impl NotationPatternDef {
    /// Parse a candidate string into its components, or return the
    /// specific reason it failed.
    ///
    /// `code_resolver` is a closure that, for a `CodeFromSet`
    /// component, returns `true` if the token is an allowed code
    /// in the referenced value set. The indirection keeps this
    /// module free of IR-level dependencies so it stays testable
    /// in isolation; the IR-level wrapper wires the closure to
    /// [`crate::value_set::expand_value_set`].
    pub fn parse<F>(
        &self,
        value: &str,
        mut code_resolver: F,
    ) -> Result<Vec<ParsedComponent>, NotationError>
    where
        F: FnMut(&ValueSetId, &str) -> bool,
    {
        // Split by separator (empty separator = fixed-width
        // concatenation, handled by a fixed-width walker).
        let tokens: Vec<&str> = if self.separator.is_empty() {
            split_fixed_width(value, &self.components)
        } else {
            value.split(self.separator.as_str()).collect()
        };

        if tokens.len() != self.components.len() {
            return Err(NotationError::ComponentCount {
                expected: self.components.len(),
                found: tokens.len(),
                separator: self.separator.clone(),
            });
        }

        let mut out = Vec::with_capacity(self.components.len());
        for (component, token) in self.components.iter().zip(tokens.iter()) {
            let parsed = parse_component(component, token, &mut code_resolver)?;
            out.push(parsed);
        }
        Ok(out)
    }

    /// Validate without materialising the components — cheaper than
    /// `parse` when the caller only needs the yes/no answer.
    pub fn validate<F>(&self, value: &str, code_resolver: F) -> Result<(), NotationError>
    where
        F: FnMut(&ValueSetId, &str) -> bool,
    {
        self.parse(value, code_resolver).map(|_| ())
    }

    /// Render a set of component values into the canonical notation
    /// string. Component order is driven by `self.components`; the
    /// caller supplies the values by name.
    ///
    /// Missing components fall back to a short error listing the
    /// missing names. Out-of-range integers, non-allowed codes,
    /// and width mismatches all surface as `NotationError` — the
    /// same type parse uses, so callers have one error surface to
    /// handle.
    pub fn render<F>(
        &self,
        values: &std::collections::HashMap<String, RenderValue>,
        mut code_resolver: F,
    ) -> Result<String, NotationError>
    where
        F: FnMut(&ValueSetId, &str) -> bool,
    {
        let mut parts = Vec::with_capacity(self.components.len());
        for component in &self.components {
            let value = values.get(&component.name).ok_or_else(|| {
                NotationError::ComponentCount {
                    expected: self.components.len(),
                    found: values.len(),
                    separator: self.separator.clone(),
                }
            })?;
            parts.push(render_component(component, value, &mut code_resolver)?);
        }
        Ok(parts.join(self.separator.as_str()))
    }
}

/// Value supplied to [`NotationPatternDef::render`]. Distinct from
/// [`ParsedValue`] because render takes the caller-owned form
/// (typed ints, raw codes) whereas parse returns both typed and
/// raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderValue {
    Code(String),
    Integer(i64),
    Alphanumeric(String),
    Text(String),
}

fn parse_component<F>(
    component: &NotationComponent,
    token: &str,
    code_resolver: &mut F,
) -> Result<ParsedComponent, NotationError>
where
    F: FnMut(&ValueSetId, &str) -> bool,
{
    let typed = match &component.kind {
        NotationComponentKind::CodeFromSet { value_set_id } => {
            if !code_resolver(value_set_id, token) {
                return Err(NotationError::UnknownCode {
                    component: component.name.clone(),
                    got: token.to_string(),
                });
            }
            ParsedValue::Code(token.to_string())
        }
        NotationComponentKind::IntegerRange { min, max, width } => {
            if *width > 0 && token.len() != *width as usize {
                return Err(NotationError::IntegerWidthMismatch {
                    component: component.name.clone(),
                    width: *width,
                    got: token.to_string(),
                });
            }
            if *width > 0 && !token.bytes().all(|b| b.is_ascii_digit()) {
                return Err(NotationError::IntegerNonDigit {
                    component: component.name.clone(),
                    width: *width,
                    got: token.to_string(),
                });
            }
            let n: i64 = token.parse().map_err(|_| NotationError::IntegerOutOfRange {
                component: component.name.clone(),
                min: *min,
                max: *max,
                got: token.to_string(),
            })?;
            if n < *min || n > *max {
                return Err(NotationError::IntegerOutOfRange {
                    component: component.name.clone(),
                    min: *min,
                    max: *max,
                    got: token.to_string(),
                });
            }
            ParsedValue::Integer(n)
        }
        NotationComponentKind::Alphanumeric { width, uppercase } => {
            if token.chars().count() != *width as usize {
                return Err(NotationError::AlphanumericWidthMismatch {
                    component: component.name.clone(),
                    width: *width,
                    got: token.to_string(),
                });
            }
            if !token.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(NotationError::AlphanumericInvalid {
                    component: component.name.clone(),
                    got: token.to_string(),
                });
            }
            if *uppercase && token.chars().any(|c| c.is_ascii_lowercase()) {
                return Err(NotationError::AlphanumericInvalid {
                    component: component.name.clone(),
                    got: token.to_string(),
                });
            }
            ParsedValue::Alphanumeric(token.to_string())
        }
        NotationComponentKind::FreeText { max_len } => {
            if let Some(max) = max_len
                && token.chars().count() > *max as usize
            {
                return Err(NotationError::FreeTextTooLong {
                    component: component.name.clone(),
                    max: *max,
                    got: token.chars().count(),
                });
            }
            ParsedValue::Text(token.to_string())
        }
    };
    Ok(ParsedComponent {
        name: component.name.clone(),
        raw: token.to_string(),
        typed,
    })
}

fn render_component<F>(
    component: &NotationComponent,
    value: &RenderValue,
    code_resolver: &mut F,
) -> Result<String, NotationError>
where
    F: FnMut(&ValueSetId, &str) -> bool,
{
    match (&component.kind, value) {
        (NotationComponentKind::CodeFromSet { value_set_id }, RenderValue::Code(c)) => {
            if !code_resolver(value_set_id, c) {
                return Err(NotationError::UnknownCode {
                    component: component.name.clone(),
                    got: c.clone(),
                });
            }
            Ok(c.clone())
        }
        (
            NotationComponentKind::IntegerRange { min, max, width },
            RenderValue::Integer(n),
        ) => {
            if n < min || n > max {
                return Err(NotationError::IntegerOutOfRange {
                    component: component.name.clone(),
                    min: *min,
                    max: *max,
                    got: n.to_string(),
                });
            }
            Ok(if *width > 0 {
                format!("{:0>width$}", n, width = *width as usize)
            } else {
                n.to_string()
            })
        }
        (
            NotationComponentKind::Alphanumeric { width, uppercase },
            RenderValue::Alphanumeric(s),
        ) => {
            if s.chars().count() != *width as usize {
                return Err(NotationError::AlphanumericWidthMismatch {
                    component: component.name.clone(),
                    width: *width,
                    got: s.clone(),
                });
            }
            if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(NotationError::AlphanumericInvalid {
                    component: component.name.clone(),
                    got: s.clone(),
                });
            }
            Ok(if *uppercase { s.to_ascii_uppercase() } else { s.clone() })
        }
        (NotationComponentKind::FreeText { max_len }, RenderValue::Text(s)) => {
            if let Some(max) = max_len
                && s.chars().count() > *max as usize
            {
                return Err(NotationError::FreeTextTooLong {
                    component: component.name.clone(),
                    max: *max,
                    got: s.chars().count(),
                });
            }
            Ok(s.clone())
        }
        _ => Err(NotationError::UnknownCode {
            component: component.name.clone(),
            got: format!("type-mismatched render value: {value:?}"),
        }),
    }
}

/// Fixed-width token splitter for components with an empty
/// separator. Each component must have a width that
/// [`component_fixed_width`] can determine; otherwise the
/// function splits as best as it can and downstream parse reports
/// a width mismatch. `FreeText` without a `max_len` is forbidden
/// at the end of an empty-separator pattern — ambiguous.
fn split_fixed_width<'a>(
    value: &'a str,
    components: &[NotationComponent],
) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(components.len());
    let mut cursor = 0usize;
    let bytes = value.len();
    for component in components {
        let width = component_fixed_width(component);
        let end = cursor.saturating_add(width).min(bytes);
        if cursor >= bytes {
            out.push("");
        } else {
            out.push(&value[cursor..end]);
            cursor = end;
        }
    }
    // Extra trailing text becomes an extra token so the component
    // count check catches the mismatch.
    if cursor < bytes {
        out.push(&value[cursor..]);
    }
    out
}

fn component_fixed_width(c: &NotationComponent) -> usize {
    match &c.kind {
        NotationComponentKind::IntegerRange { width, .. } => *width as usize,
        NotationComponentKind::Alphanumeric { width, .. } => *width as usize,
        // Code-from-set and free-text without bounds can't be
        // fixed-width split; return 0 and let the component-count
        // check surface the issue.
        _ => 0,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn pattern() -> NotationPatternDef {
        NotationPatternDef {
            id: NotationPatternId::new("np-campaign"),
            name: "CampaignCode".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            template: "{campaign}_{year:%02d}_{seq:%03d}".into(),
            separator: "_".into(),
            components: vec![
                NotationComponent {
                    name: "campaign".into(),
                    display: LocalizedText::default(),
                    kind: NotationComponentKind::CodeFromSet {
                        value_set_id: ValueSetId::new("vs-seasons"),
                    },
                },
                NotationComponent {
                    name: "year".into(),
                    display: LocalizedText::default(),
                    kind: NotationComponentKind::IntegerRange {
                        min: 0,
                        max: 99,
                        width: 2,
                    },
                },
                NotationComponent {
                    name: "seq".into(),
                    display: LocalizedText::default(),
                    kind: NotationComponentKind::IntegerRange {
                        min: 1,
                        max: 999,
                        width: 3,
                    },
                },
            ],
            examples: vec!["SPRING_26_001".into()],
        }
    }

    fn allow_seasons(set: &ValueSetId, code: &str) -> bool {
        set.as_str() == "vs-seasons"
            && matches!(code, "SPRING" | "SUMMER" | "FALL" | "WINTER")
    }

    #[test]
    fn parse_decomposes_the_user_example_spring_26_001() {
        let p = pattern();
        let parsed = p.parse("SPRING_26_001", allow_seasons).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "campaign");
        assert_eq!(parsed[0].typed, ParsedValue::Code("SPRING".into()));
        assert_eq!(parsed[1].typed, ParsedValue::Integer(26));
        assert_eq!(parsed[2].typed, ParsedValue::Integer(1));
    }

    #[test]
    fn parse_rejects_unknown_campaign_code() {
        let p = pattern();
        let err = p.parse("AUTUMN_26_001", allow_seasons).unwrap_err();
        assert!(matches!(err, NotationError::UnknownCode { .. }));
    }

    #[test]
    fn parse_rejects_out_of_range_integer() {
        let p = pattern();
        // year has min=0, max=99, width=2; "99" is valid, "AA" isn't
        let err = p.parse("SPRING_AA_001", allow_seasons).unwrap_err();
        assert!(matches!(err, NotationError::IntegerNonDigit { .. }));
    }

    #[test]
    fn parse_rejects_wrong_width_integer() {
        let p = pattern();
        let err = p.parse("SPRING_1_001", allow_seasons).unwrap_err();
        assert!(matches!(err, NotationError::IntegerWidthMismatch { .. }));
    }

    #[test]
    fn parse_rejects_wrong_component_count() {
        let p = pattern();
        let err = p.parse("SPRING_26", allow_seasons).unwrap_err();
        assert!(matches!(err, NotationError::ComponentCount { .. }));
    }

    #[test]
    fn render_emits_canonical_form_with_padding() {
        let p = pattern();
        let mut values = HashMap::new();
        values.insert("campaign".into(), RenderValue::Code("FALL".into()));
        values.insert("year".into(), RenderValue::Integer(26));
        values.insert("seq".into(), RenderValue::Integer(42));
        let s = p.render(&values, allow_seasons).unwrap();
        assert_eq!(s, "FALL_26_042");
    }

    #[test]
    fn parse_render_round_trip_preserves_semantics() {
        let p = pattern();
        let original = "WINTER_26_007";
        let parsed = p.parse(original, allow_seasons).unwrap();
        let mut values = HashMap::new();
        for c in &parsed {
            let rv = match &c.typed {
                ParsedValue::Code(s) => RenderValue::Code(s.clone()),
                ParsedValue::Integer(n) => RenderValue::Integer(*n),
                ParsedValue::Alphanumeric(s) => RenderValue::Alphanumeric(s.clone()),
                ParsedValue::Text(s) => RenderValue::Text(s.clone()),
            };
            values.insert(c.name.clone(), rv);
        }
        let rendered = p.render(&values, allow_seasons).unwrap();
        assert_eq!(rendered, original);
    }

    #[test]
    fn empty_separator_splits_on_fixed_widths() {
        // Korean 민번 shape stripped: YYMMDD-NNNNNNN split on -, but
        // test the no-separator case with a 6+7 format:
        // "260315" = YYMMDD (6) then split.
        let p = NotationPatternDef {
            id: NotationPatternId::new("np-date6"),
            name: "date6".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            template: "{yy}{mmdd}".into(),
            separator: String::new(),
            components: vec![
                NotationComponent {
                    name: "yy".into(),
                    display: LocalizedText::default(),
                    kind: NotationComponentKind::IntegerRange {
                        min: 0,
                        max: 99,
                        width: 2,
                    },
                },
                NotationComponent {
                    name: "mmdd".into(),
                    display: LocalizedText::default(),
                    kind: NotationComponentKind::IntegerRange {
                        min: 101,
                        max: 1231,
                        width: 4,
                    },
                },
            ],
            examples: vec!["260315".into()],
        };
        let parsed = p.parse("260315", |_, _| true).unwrap();
        assert_eq!(parsed[0].typed, ParsedValue::Integer(26));
        assert_eq!(parsed[1].typed, ParsedValue::Integer(315));
    }

    #[test]
    fn alphanumeric_uppercase_enforced() {
        let p = NotationPatternDef {
            id: NotationPatternId::new("np-ap"),
            name: "AlphaPrefix".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            template: "{p}".into(),
            separator: String::new(),
            components: vec![NotationComponent {
                name: "p".into(),
                display: LocalizedText::default(),
                kind: NotationComponentKind::Alphanumeric {
                    width: 3,
                    uppercase: true,
                },
            }],
            examples: vec![],
        };
        let err = p.parse("abc", |_, _| true).unwrap_err();
        assert!(matches!(err, NotationError::AlphanumericInvalid { .. }));
        assert!(p.parse("ABC", |_, _| true).is_ok());
    }

    #[test]
    fn notation_pattern_round_trips_through_json() {
        let p = pattern();
        let j = serde_json::to_value(&p).unwrap();
        let back: NotationPatternDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, p);
    }
}

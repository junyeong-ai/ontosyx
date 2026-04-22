//! Code systems and coded values — terminology registry foundation.
//!
//! A [`CodeSystemDef`] declares a named set of codes with
//! meanings. The nested [`CodedValue`] carries one code's
//! identifier, display label, definition, synonyms, optional
//! broader / replacement relationships, and temporal validity.
//!
//! This is the foundation every downstream terminology surface
//! (value sets, concept maps, notation patterns, units of measure,
//! numeric range bands) builds on. Without it, coded columns like
//! `order.status = 'A'` are opaque to the LLM prompt path, data
//! quality rules, and the navigation UI.
//!
//! ## Conceptual reference
//!
//! The shape deliberately converges on three industry-standard
//! models so a later mapping / import / export layer is trivial:
//!
//! - **HL7 FHIR R5 CodeSystem** — `id`, `url` (our `uri`),
//!   `version`, `content`, `concept[]`. We pre-nest `concept[]`
//!   as [`CodedValue`] exactly because FHIR does (DDD aggregate).
//! - **ISO/IEC 11179 Part 3** — Value Meaning / Permissible Value.
//!   A [`CodedValue`] is both: `code` is the permissible value,
//!   `display` + `definition` + `scope_note` are the value meaning.
//! - **W3C SKOS** — `broader_id` is `skos:broader`; `aliases` is
//!   `skos:altLabel`; `scope_note` is `skos:scopeNote`; `examples`
//!   is `skos:example`.
//!
//! ## DDD aggregate
//!
//! [`CodedValue`] is nested inside [`CodeSystemDef::codes`] rather
//! than being a top-level collection — the system is the aggregate
//! root, the codes have no meaning without the system. Matches the
//! existing [`crate::glossary::TaxonomyNode`] ⊂ [`crate::glossary::TaxonomyDef`]
//! precedent.
//!
//! The [`CodedValueId`] newtype still exists because downstream
//! types ([`crate::mapping`], `PropertyDef::unit_id`) reference a
//! specific code across systems; the id is system-independent and
//! must be globally unique inside an `OntologyIR`.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`CodeSystemDef`].
    CodeSystemId
);

ox_core::define_id_newtype!(
    /// Stable identifier for a [`CodedValue`]. Globally unique
    /// inside an `OntologyIR` — two systems cannot share a code's
    /// id even if they share its `code` string.
    CodedValueId
);

/// A named, versioned set of codes that together carry semantic
/// meaning for one domain concept.
///
/// A `CodeSystemDef` is the terminology equivalent of a typed
/// enumeration. The `uri` field is how two systems with the same
/// short `name` stay distinguishable — "A" in
/// `urn:ox:order-status` and "A" in `urn:ox:customer-tier` are
/// distinct codes even though their `code` strings collide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CodeSystemDef {
    pub id: CodeSystemId,

    /// Short human identifier (e.g. `"OrderStatus"`). Not required
    /// to be Cypher-safe; this is display / lookup text only.
    pub name: String,

    /// Localized display name shown in the admin UI.
    #[serde(default)]
    pub display_name: LocalizedText,

    /// Localized description — longer than `name`, used in admin UIs
    /// and injected into LLM prompt context.
    #[serde(default)]
    pub description: LocalizedText,

    /// URI that globally identifies the system. For internal systems
    /// use `urn:ox:<tenant>:<name>`; for external systems point at
    /// the authoritative registry (`http://unitsofmeasure.org`,
    /// `urn:iso:std:iso:3166`, etc.). Optional — plain `name` is
    /// enough for small deployments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    /// Version string. Semver-ish but free-form — external systems
    /// like ICD-10 have their own versioning scheme ("2019", "2024").
    pub version: String,

    /// Governance kind. [`CodeSystemKind::External`] systems must
    /// not be edited through the admin UI — they are mirrors of an
    /// authoritative registry and their codes must round-trip.
    pub kind: CodeSystemKind,

    /// Whether codes form a hierarchy via
    /// [`CodedValue::broader_id`]. `true` unlocks descendants-of /
    /// narrower-than queries; `false` keeps the system flat
    /// (catalogue shape). A code with `broader_id` set in a flat
    /// system is a validation error.
    #[serde(default)]
    pub hierarchical: bool,

    /// The codes themselves — DDD aggregate (nested).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<CodedValue>,

    /// Deprecation timestamp for the whole system. A superseded
    /// system still round-trips but the admin UI nudges users onto
    /// `replaced_by_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<DateTime<Utc>>,

    /// Successor system when this one is deprecated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<CodeSystemId>,
}

/// Governance kind of a [`CodeSystemDef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodeSystemKind {
    /// Tenant-editable business codes. Admin UI can add / remove /
    /// rename entries. Typical for campaign codes, customer
    /// segments, internal process states.
    Internal,
    /// Immutable mirror of an external authoritative registry
    /// (ISO, HL7, SNOMED, UCUM, internal legacy). The admin UI
    /// treats codes as read-only; imports keep the local copy in
    /// sync but never modify in-place.
    External {
        /// Canonical source reference — URL, OID, or registry name
        /// (`"http://unitsofmeasure.org"`, `"urn:iso:std:iso:3166"`,
        /// `"urn:legacy-crm:cust-status"`).
        source_ref: String,
    },
}

/// One code in a [`CodeSystemDef`]. Carries the raw `code` plus
/// localized display / definition + synonyms, hierarchy parent,
/// examples, and temporal validity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CodedValue {
    pub id: CodedValueId,

    /// The canonical code (`"A"`, `"SPRING"`, `"KR"`, `"kg"`). Not
    /// localized — codes are the invariant; display names vary by
    /// locale. Case-sensitive; the admin is expected to pick a
    /// consistent convention.
    pub code: String,

    /// Localized display label (`{en: "Active", ko: "활성"}`).
    #[serde(default)]
    pub display: LocalizedText,

    /// Localized definition — the precise meaning of the code.
    /// Separate from `scope_note`, which carries usage guidance.
    #[serde(default)]
    pub definition: LocalizedText,

    /// Alternate spellings / short codes / legacy synonyms used for
    /// lookup. The admin UI index + LLM prompt expansion both read
    /// this so a user query for `"ACT"` resolves to `"A"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// Parent in the system's hierarchy — only meaningful when the
    /// owning system has `hierarchical: true`. Validation rejects a
    /// `broader_id` pointing into a different system and rejects
    /// cycles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broader_id: Option<CodedValueId>,

    /// Curated representative examples (SKOS `skos:example`).
    /// Distinct from the profiler-emitted `ColumnStats.sample_values`:
    /// these are *authored*, not observed, so the LLM gets stable
    /// anchors that do not drift as data changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<LocalizedText>,

    /// SKOS `skos:scopeNote` — when *should* this code be used,
    /// what's the typical misuse to avoid. Longer than `definition`,
    /// shown in the admin UI tooltip.
    #[serde(default)]
    pub scope_note: LocalizedText,

    /// Start of the code's validity window. `None` means "since
    /// inception". Used by temporal queries that need "what was
    /// valid on 2024-01-01".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,

    /// End of the code's validity window. `None` means "currently
    /// valid". Distinct from `deprecated_at`: a code can be valid
    /// but deprecated (still in use, new registrations discouraged)
    /// or invalid (no new uses accepted at all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,

    /// Deprecation marker — set when the code is soft-retired.
    /// Admin UI grays out; new registrations rejected unless
    /// `Severity::Warn` flag is acknowledged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<DateTime<Utc>>,

    /// Replacement code for deprecated entries. Runtime mappers
    /// follow the chain transparently when consumers ask for a
    /// deprecated code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<CodedValueId>,
}

impl CodeSystemDef {
    /// O(n) lookup by `code` string. Tests and small in-memory
    /// flows use this directly; the `OntologyIR`-level
    /// `coded_value_by_id` builds a HashMap index for O(1).
    pub fn find_code(&self, code: &str) -> Option<&CodedValue> {
        self.codes.iter().find(|cv| cv.code == code)
    }

    /// Returns `true` if any code in this system carries the
    /// supplied alias (case-insensitive, trimmed). Alias lookup is
    /// the fallback when a user types `"ACT"` expecting `"A"`.
    pub fn alias_matches(&self, text: &str) -> Option<&CodedValue> {
        let needle = text.trim().to_ascii_lowercase();
        self.codes.iter().find(|cv| {
            cv.code.to_ascii_lowercase() == needle
                || cv.aliases.iter().any(|a| a.to_ascii_lowercase() == needle)
        })
    }
}

impl CodedValue {
    /// Is the code active at the supplied instant? Checks both
    /// validity window and deprecation.
    pub fn is_active_at(&self, t: DateTime<Utc>) -> bool {
        if let Some(dep) = self.deprecated_at
            && dep <= t
        {
            return false;
        }
        if let Some(from) = self.valid_from
            && t < from
        {
            return false;
        }
        if let Some(to) = self.valid_to
            && t >= to
        {
            return false;
        }
        true
    }
}

// ---------------------------------------------------------------------------
// UCUM seed — minimal reference-data set for PropertyDef.unit_id
// ---------------------------------------------------------------------------
//
// Full UCUM covers thousands of codes (http://unitsofmeasure.org).
// We seed only the subset every deployment needs out of the box
// (mass · length · time · frequency · currency-like). A tenant
// that needs the full table imports UCUM separately; this helper
// just removes the friction of "the first Property with a unit
// can't compile because no UCUM CodeSystem exists yet."

/// Build a minimal UCUM [`CodeSystemDef`] covering the most common
/// units authored in typical property schemas. Callers assign the
/// `id` / `version` they want at insert time — reusing a canonical
/// shape rather than autogenerating keeps tenant-level upgrades
/// deterministic.
///
/// This is a deliberately **small** subset (19 entries). A full
/// UCUM registry import is a separate admin workflow; this seed
/// is the "unblock Property.unit_id on day one" helper.
pub fn ucum_seed(id: CodeSystemId, version: String) -> CodeSystemDef {
    use ox_core::i18n::LanguageTag;

    fn tag(s: &'static str) -> LanguageTag {
        // Compile-time constants — the only failure mode would be a
        // programmer typo in the seed itself, not runtime data.
        #[allow(clippy::expect_used)]
        LanguageTag::parse(s).expect("seed LanguageTag literal must parse")
    }

    let code = |short: &str, disp_en: &'static str, disp_ko: &'static str| CodedValue {
        id: CodedValueId::new(format!("ucum-{short}")),
        code: short.to_string(),
        display: LocalizedText::new(disp_en)
            .with_translation(tag("en"), disp_en)
            .with_translation(tag("ko"), disp_ko),
        definition: LocalizedText::default(),
        aliases: Vec::new(),
        broader_id: None,
        examples: Vec::new(),
        scope_note: LocalizedText::default(),
        valid_from: None,
        valid_to: None,
        deprecated_at: None,
        replaced_by_id: None,
    };

    CodeSystemDef {
        id,
        name: "UCUM".into(),
        display_name: LocalizedText::new("Unified Code for Units of Measure")
            .with_translation(tag("ko"), "단위 표준 코드 (UCUM)"),
        description: LocalizedText::default(),
        uri: Some("http://unitsofmeasure.org".into()),
        version,
        kind: CodeSystemKind::External {
            source_ref: "http://unitsofmeasure.org".into(),
        },
        hierarchical: false,
        codes: vec![
            // Mass
            code("g", "gram", "그램"),
            code("kg", "kilogram", "킬로그램"),
            code("mg", "milligram", "밀리그램"),
            // Length
            code("m", "meter", "미터"),
            code("km", "kilometer", "킬로미터"),
            code("cm", "centimeter", "센티미터"),
            code("mm", "millimeter", "밀리미터"),
            // Time
            code("s", "second", "초"),
            code("min", "minute", "분"),
            code("h", "hour", "시간"),
            code("d", "day", "일"),
            // Frequency
            code("Hz", "hertz", "헤르츠"),
            // Volume
            code("L", "liter", "리터"),
            code("mL", "milliliter", "밀리리터"),
            // Currency (ISO 4217 — UCUM passes through as `{code}`)
            code("USD", "US dollar", "미국 달러"),
            code("KRW", "Korean won", "대한민국 원"),
            code("EUR", "euro", "유로"),
            code("JPY", "Japanese yen", "일본 엔"),
            code("CNY", "Chinese yuan", "중국 위안"),
        ],
        deprecated_at: None,
        replaced_by_id: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sys(id: &str, name: &str, kind: CodeSystemKind) -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new(id),
            name: name.into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind,
            hierarchical: false,
            codes: Vec::new(),
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn code(id: &str, c: &str) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: c.into(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: Vec::new(),
            broader_id: None,
            examples: Vec::new(),
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    #[test]
    fn find_code_returns_the_matching_entry() {
        let mut s = sys("cs-1", "OrderStatus", CodeSystemKind::Internal);
        s.codes.push(code("cv-a", "A"));
        s.codes.push(code("cv-c", "C"));
        assert_eq!(s.find_code("A").unwrap().id, CodedValueId::new("cv-a"));
        assert!(s.find_code("X").is_none());
    }

    #[test]
    fn alias_matches_is_case_insensitive_and_checks_code_plus_aliases() {
        let mut s = sys("cs-1", "OrderStatus", CodeSystemKind::Internal);
        let mut cv = code("cv-a", "A");
        cv.aliases = vec!["ACT".into(), "Active".into()];
        s.codes.push(cv);
        assert_eq!(s.alias_matches("a").unwrap().id, CodedValueId::new("cv-a"));
        assert_eq!(s.alias_matches(" ACT ").unwrap().id, CodedValueId::new("cv-a"));
        assert_eq!(
            s.alias_matches("active").unwrap().id,
            CodedValueId::new("cv-a")
        );
        assert!(s.alias_matches("xyz").is_none());
    }

    #[test]
    fn is_active_at_respects_deprecation_and_validity_window() {
        let t0 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t_mid = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let t_end = Utc.with_ymd_and_hms(2024, 12, 31, 0, 0, 0).unwrap();
        let mut cv = code("cv-1", "X");
        cv.valid_from = Some(t0);
        cv.valid_to = Some(t_end);
        assert!(cv.is_active_at(t_mid));
        assert!(!cv.is_active_at(t0 - chrono::Duration::days(1)));
        assert!(!cv.is_active_at(t_end));

        cv.deprecated_at = Some(t_mid);
        assert!(!cv.is_active_at(t_mid));
        assert!(cv.is_active_at(t0 + chrono::Duration::days(1)));
    }

    #[test]
    fn external_system_carries_authoritative_source_ref() {
        let s = sys(
            "cs-ucum",
            "UCUM",
            CodeSystemKind::External {
                source_ref: "http://unitsofmeasure.org".into(),
            },
        );
        match s.kind {
            CodeSystemKind::External { ref source_ref } => {
                assert_eq!(source_ref, "http://unitsofmeasure.org");
            }
            _ => panic!("expected External"),
        }
    }

    #[test]
    fn ucum_seed_carries_canonical_subset_and_external_source_ref() {
        let sys = ucum_seed(CodeSystemId::new("cs-ucum"), "2024".into());
        assert_eq!(sys.name, "UCUM");
        assert!(!sys.hierarchical);
        match sys.kind {
            CodeSystemKind::External { ref source_ref } => {
                assert_eq!(source_ref, "http://unitsofmeasure.org");
            }
            _ => panic!("UCUM seed must be External"),
        }
        // Mass / length / time / frequency / volume / currency
        // representatives present.
        for expected in ["kg", "m", "s", "Hz", "L", "KRW", "USD"] {
            assert!(
                sys.find_code(expected).is_some(),
                "UCUM seed missing '{expected}'"
            );
        }
        // Display localization populated for at least one locale.
        let kg = sys.find_code("kg").unwrap();
        assert!(!kg.display.is_empty());
    }

    #[test]
    fn code_system_round_trips_through_json() {
        let mut s = sys("cs-1", "OrderStatus", CodeSystemKind::Internal);
        s.hierarchical = true;
        let mut cv = code("cv-a", "A");
        cv.aliases = vec!["ACT".into()];
        cv.broader_id = Some(CodedValueId::new("cv-root"));
        s.codes.push(cv);

        let j = serde_json::to_value(&s).unwrap();
        let back: CodeSystemDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, s);
    }
}

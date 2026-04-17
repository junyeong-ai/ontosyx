//! `PromptVersion` — semantic version for prompt templates.
//!
//! Lives in `ox-core` so both `ox-store` (column type for
//! `prompt_templates.version`) and `ox-brain` (registry-level enforcement)
//! agree on the same parsed value.
//!
//! Wire format: `"major.minor.patch"`. JSON serialization round-trips as
//! a string so OpenAPI consumers see a familiar shape; the type itself is
//! `Ord` so callers can sort or compare versions semantically without
//! the string `"v10" < "v9"` lexicographic surprise.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{OxError, OxResult};

/// Parsed semantic version (major.minor.patch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PromptVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PromptVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse a `"major.minor.patch"` version string.
    ///
    /// Returns `OxError::Validation` for any malformed input — the caller
    /// can map this to a domain-appropriate error.
    pub fn parse(version: &str) -> OxResult<Self> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(OxError::Validation {
                field: "version".to_string(),
                message: format!(
                    "Invalid prompt version '{version}': expected major.minor.patch"
                ),
            });
        }
        let major = parts[0].parse::<u32>().map_err(|_| OxError::Validation {
            field: "version".to_string(),
            message: format!("Invalid major version in '{version}'"),
        })?;
        let minor = parts[1].parse::<u32>().map_err(|_| OxError::Validation {
            field: "version".to_string(),
            message: format!("Invalid minor version in '{version}'"),
        })?;
        let patch = parts[2].parse::<u32>().map_err(|_| OxError::Validation {
            field: "version".to_string(),
            message: format!("Invalid patch version in '{version}'"),
        })?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for PromptVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for PromptVersion {
    type Err = OxError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// `TryFrom<String>` so `#[sqlx(try_from = "String")]` works on the
// `PromptTemplateRow.version` column without bespoke Encode/Decode impls.
impl TryFrom<String> for PromptVersion {
    type Error = OxError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<PromptVersion> for String {
    fn from(v: PromptVersion) -> Self {
        v.to_string()
    }
}

// JSON: serialize as the canonical string so OpenAPI / generated TS
// types see `"1.2.3"` rather than `{"major":1,"minor":2,"patch":3}`.
impl Serialize for PromptVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PromptVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotted_triple() {
        let v = PromptVersion::parse("2.1.5").unwrap();
        assert_eq!(v, PromptVersion::new(2, 1, 5));
    }

    #[test]
    fn rejects_bad_shape() {
        assert!(PromptVersion::parse("1.0").is_err());
        assert!(PromptVersion::parse("1.0.0.0").is_err());
        assert!(PromptVersion::parse("v1.0.0").is_err());
        assert!(PromptVersion::parse("abc").is_err());
        assert!(PromptVersion::parse("").is_err());
    }

    #[test]
    fn ord_is_semver_not_lexicographic() {
        // The whole point: "v10" lex-sorts before "v9" but
        // PromptVersion sorts as 9.0.0 < 10.0.0.
        let v9 = PromptVersion::parse("9.0.0").unwrap();
        let v10 = PromptVersion::parse("10.0.0").unwrap();
        assert!(v9 < v10);
        assert!(v10 > v9);

        let v_2_1 = PromptVersion::parse("2.1.0").unwrap();
        let v_2_10 = PromptVersion::parse("2.10.0").unwrap();
        assert!(v_2_1 < v_2_10);
    }

    #[test]
    fn display_round_trips() {
        let cases = ["0.0.0", "1.2.3", "100.200.300"];
        for c in cases {
            let parsed = PromptVersion::parse(c).unwrap();
            assert_eq!(parsed.to_string(), c);
        }
    }

    #[test]
    fn json_serializes_as_string() {
        let v = PromptVersion::new(1, 2, 3);
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, "\"1.2.3\"");
        let back: PromptVersion = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn try_from_string_works() {
        let v: PromptVersion = "1.2.3".to_string().try_into().unwrap();
        assert_eq!(v, PromptVersion::new(1, 2, 3));
        let s: String = v.into();
        assert_eq!(s, "1.2.3");
    }
}

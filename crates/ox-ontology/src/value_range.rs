//! Value range sets — numeric interpretive bands.
//!
//! A [`ValueRangeSetDef`] declares named bands over the numeric
//! axis of a property: blood pressure `{90-120 Normal, 120-140
//! Elevated, 140+ High}`, cost `{< warning / < critical / >=
//! critical}`, age `{< child / < adult / >= senior}`.
//!
//! This is the numeric equivalent of
//! [`crate::value_set::ValueSetDef`] — where value sets carve
//! discrete codes into an allowed subset, value range sets carve
//! a continuous range into labeled intervals.
//!
//! ## Conceptual reference
//!
//! - **HL7 FHIR Observation.referenceRange** — the canonical
//!   clinical use case: a lab result's interpretation bands
//!   (`low` / `normal` / `high`).
//! - **OGC Observations & Measurements** — range classification.
//! - **Everyday BI** — "good / warning / critical" thresholds on
//!   cost / latency / availability metrics.
//!
//! ## Semantics
//!
//! Bands carry:
//! - Optional `min` (`None` = -∞)
//! - Optional `max` (`None` = +∞)
//! - Inclusivity flags per endpoint
//! - Localized label
//! - Optional [`crate::rule::Severity`] for colour / alerting
//!
//! A [`ValueRangeSetDef`] is **authored non-overlapping** — the
//! admin UI enforces this, and [`ValueRangeSetDef::classify`]
//! returns the first matching band in declaration order so an
//! overlap (if it slips through validation) resolves
//! deterministically. Gaps between bands are legal; a value that
//! falls in a gap returns `None`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::rule::Severity;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`ValueRangeSetDef`].
    ValueRangeSetId
);

/// A named set of numeric interpretive bands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValueRangeSetDef {
    pub id: ValueRangeSetId,
    pub name: String,

    #[serde(default)]
    pub display_name: LocalizedText,

    #[serde(default)]
    pub description: LocalizedText,

    pub version: String,

    /// Ordered bands. Authored non-overlapping; the declaration
    /// order drives tie-breaking on any accidental overlap and
    /// drives UI rendering order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bands: Vec<ValueBand>,
}

/// One band — an interval on the numeric axis plus an
/// interpretation label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValueBand {
    /// Lower bound. `None` means -∞.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Upper bound. `None` means +∞.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Whether `min` is inclusive. Ignored when `min.is_none()`.
    #[serde(default)]
    pub inclusive_min: bool,
    /// Whether `max` is inclusive. Ignored when `max.is_none()`.
    #[serde(default)]
    pub inclusive_max: bool,
    /// Localized display label (`"Normal"` / `"정상"`).
    #[serde(default)]
    pub label: LocalizedText,
    /// Colour / alerting hint. Reuses [`Severity`] from the
    /// governance layer so UIs already wired for rule severities
    /// render range bands with the same palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
}

impl ValueBand {
    /// `true` when `value` falls in this band according to the
    /// inclusivity flags.
    pub fn contains(&self, value: f64) -> bool {
        if let Some(min) = self.min {
            let above = if self.inclusive_min {
                value >= min
            } else {
                value > min
            };
            if !above {
                return false;
            }
        }
        if let Some(max) = self.max {
            let below = if self.inclusive_max {
                value <= max
            } else {
                value < max
            };
            if !below {
                return false;
            }
        }
        true
    }
}

impl ValueRangeSetDef {
    /// Return the first band that contains `value`, or `None` when
    /// no band matches (legal — gaps between bands are allowed).
    pub fn classify(&self, value: f64) -> Option<&ValueBand> {
        self.bands.iter().find(|b| b.contains(value))
    }

    /// Diagnostic: scan for overlapping bands. Authoring tools
    /// call this pre-save to surface accidental overlaps — the
    /// runtime `classify` still works deterministically (first
    /// match wins) but the operator almost certainly did not mean
    /// it.
    pub fn find_overlaps(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for i in 0..self.bands.len() {
            for j in (i + 1)..self.bands.len() {
                if bands_overlap(&self.bands[i], &self.bands[j]) {
                    out.push((i, j));
                }
            }
        }
        out
    }
}

fn bands_overlap(a: &ValueBand, b: &ValueBand) -> bool {
    // Two bands overlap iff BOTH:
    //   - a does NOT start strictly after b ends
    //   - b does NOT start strictly after a ends
    //
    // "a starts strictly after b ends" (no overlap from that side):
    //   am > bm                                    — strict
    //   am == bm AND (NOT a_inc OR NOT b_inc)      — equal boundary
    //     with at least one exclusive side → the shared point
    //     belongs to at most one band, no overlap
    //
    // Only when am == bm AND both are inclusive does the shared
    // point fall in both bands → overlap.
    //
    // `None` means unbounded: -∞ on the start side, +∞ on the end
    // side. `None` start / `None` end cannot be "strictly after"
    // anything on the relevant axis.
    fn starts_strictly_after_end(
        start: Option<(f64, bool)>,
        end: Option<(f64, bool)>,
    ) -> bool {
        let (Some((sm, s_inc)), Some((em, e_inc))) = (start, end) else {
            return false;
        };
        if sm > em {
            true
        } else if sm == em {
            !s_inc || !e_inc
        } else {
            false
        }
    }
    let a_start = a.min.map(|m| (m, a.inclusive_min));
    let a_end = a.max.map(|m| (m, a.inclusive_max));
    let b_start = b.min.map(|m| (m, b.inclusive_min));
    let b_end = b.max.map(|m| (m, b.inclusive_max));

    !starts_strictly_after_end(a_start, b_end)
        && !starts_strictly_after_end(b_start, a_end)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn band(min: Option<f64>, max: Option<f64>, label: &str, inc_min: bool, inc_max: bool) -> ValueBand {
        ValueBand {
            min,
            max,
            inclusive_min: inc_min,
            inclusive_max: inc_max,
            label: LocalizedText::new(label),
            severity: None,
        }
    }

    fn bp() -> ValueRangeSetDef {
        // Blood pressure systolic: <90 low, 90-120 normal,
        // 120-140 elevated, 140+ high. Boundaries inclusive on
        // the lower side — matches FHIR convention.
        ValueRangeSetDef {
            id: ValueRangeSetId::new("rs-bp"),
            name: "SystolicBP".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            bands: vec![
                band(None, Some(90.0), "Low", false, false),
                band(Some(90.0), Some(120.0), "Normal", true, false),
                band(Some(120.0), Some(140.0), "Elevated", true, false),
                band(Some(140.0), None, "High", true, false),
            ],
        }
    }

    #[test]
    fn classify_returns_correct_band() {
        let rs = bp();
        assert_eq!(rs.classify(85.0).unwrap().label.default_str(), "Low");
        assert_eq!(rs.classify(90.0).unwrap().label.default_str(), "Normal");
        assert_eq!(rs.classify(119.0).unwrap().label.default_str(), "Normal");
        assert_eq!(rs.classify(120.0).unwrap().label.default_str(), "Elevated");
        assert_eq!(rs.classify(145.0).unwrap().label.default_str(), "High");
    }

    #[test]
    fn classify_infinite_tails_work_both_sides() {
        let rs = ValueRangeSetDef {
            id: ValueRangeSetId::new("rs-inf"),
            name: "test".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            bands: vec![
                band(None, Some(0.0), "Neg", false, false),
                band(Some(0.0), None, "Pos", true, false),
            ],
        };
        assert_eq!(rs.classify(-1e9).unwrap().label.default_str(), "Neg");
        assert_eq!(rs.classify(0.0).unwrap().label.default_str(), "Pos");
        assert_eq!(rs.classify(1e9).unwrap().label.default_str(), "Pos");
    }

    #[test]
    fn classify_returns_none_for_gap() {
        let rs = ValueRangeSetDef {
            id: ValueRangeSetId::new("rs-gap"),
            name: "test".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            bands: vec![
                band(Some(0.0), Some(10.0), "A", true, true),
                band(Some(20.0), Some(30.0), "B", true, true),
            ],
        };
        assert!(rs.classify(15.0).is_none());
    }

    #[test]
    fn find_overlaps_flags_accidental_overlap() {
        let rs = ValueRangeSetDef {
            id: ValueRangeSetId::new("rs-overlap"),
            name: "test".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            bands: vec![
                band(Some(0.0), Some(10.0), "A", true, true),
                band(Some(5.0), Some(15.0), "B", true, true),
            ],
        };
        let overlaps = rs.find_overlaps();
        assert_eq!(overlaps, vec![(0, 1)]);
    }

    #[test]
    fn find_overlaps_empty_when_adjacent_exclusive_boundaries() {
        // [0,10) and [10,20) — share 10.0 but exclusive upper
        // in the first, inclusive lower in the second. Not an
        // overlap.
        let rs = ValueRangeSetDef {
            id: ValueRangeSetId::new("rs-adj"),
            name: "test".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            bands: vec![
                band(Some(0.0), Some(10.0), "A", true, false),
                band(Some(10.0), Some(20.0), "B", true, false),
            ],
        };
        assert!(rs.find_overlaps().is_empty());
        // Boundary behaviour: 10.0 belongs to B (inclusive_min),
        // not A (exclusive_max).
        assert_eq!(rs.classify(10.0).unwrap().label.default_str(), "B");
    }

    #[test]
    fn value_range_set_round_trips_through_json() {
        let rs = bp();
        let j = serde_json::to_value(&rs).unwrap();
        let back: ValueRangeSetDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, rs);
    }
}

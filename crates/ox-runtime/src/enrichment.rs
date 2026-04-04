//! Schema enrichment — programmatic sample value injection into property descriptions.
//!
//! Merges profiler data (sample values, min/max ranges, distinct counts) into
//! `PropertyDef.description` fields. This gives the LLM concrete value examples
//! for accurate query generation without any LLM cost.
//!
//! Works independently of the LLM-based `refine_ontology` pipeline — can be
//! triggered after data load, manually via API, or as a post-refinement step.

use ox_core::ontology_ir::OntologyIR;
use serde::Serialize;

use crate::profiler::{DataProfile, PropertyStats};

// ---------------------------------------------------------------------------
// Enrichment markers — used for idempotent re-enrichment
// ---------------------------------------------------------------------------

/// Enrichment marker: newline + bracketed tag. Unambiguous, no collision with
/// user descriptions (unlike the legacy ` | ` separator).
const ENRICHMENT_MARKER: &str = "\n[enriched] ";

/// Legacy separator (pre-v1). Supported in strip only for data migration.
const LEGACY_SEPARATOR: &str = " | ";

const PREFIX_VALUES: &str = "Values: ";
const PREFIX_RANGE: &str = "Range: ";
const PREFIX_EXAMPLES: &str = "Examples: ";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of enrichment: the mutated ontology + diff of what changed.
pub struct EnrichmentResult {
    pub ontology: OntologyIR,
    pub changes: Vec<PropertyEnrichment>,
}

/// A single property description change.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyEnrichment {
    pub entity_label: String,
    pub entity_kind: &'static str, // "node" or "edge"
    pub property_name: String,
    pub old_description: Option<String>,
    pub new_description: String,
}

// ---------------------------------------------------------------------------
// Core enrichment logic
// ---------------------------------------------------------------------------

/// Merge profiler results into ontology property descriptions.
///
/// - Strips previous enrichment data (idempotent — safe to call multiple times)
/// - Preserves manual descriptions, appending enrichment after ` | ` separator
/// - Processes both node and edge properties
pub fn enrich_descriptions(ontology: &OntologyIR, profile: &DataProfile) -> EnrichmentResult {
    let mut ont = ontology.clone();
    let mut changes = Vec::new();

    // Enrich node properties
    for node_profile in &profile.node_profiles {
        let Some(node) = ont.node_types.iter_mut().find(|n| n.label == node_profile.label) else {
            continue;
        };
        for stats in &node_profile.property_stats {
            let Some(prop) = node.properties.iter_mut().find(|p| p.name == stats.name) else {
                continue;
            };
            if let Some(enrichment) = format_enrichment(stats) {
                let old = prop.description.clone();
                let manual = strip_enrichment(&prop.description);
                let new_desc = match manual {
                    Some(manual) => format!("{manual}{ENRICHMENT_MARKER}{enrichment}"),
                    None => enrichment.clone(),
                };
                if prop.description.as_deref() != Some(&new_desc) {
                    changes.push(PropertyEnrichment {
                        entity_label: node_profile.label.clone(),
                        entity_kind: "node",
                        property_name: stats.name.clone(),
                        old_description: old,
                        new_description: new_desc.clone(),
                    });
                    prop.description = Some(new_desc);
                }
            }
        }
    }

    // Enrich edge properties
    for edge_profile in &profile.edge_profiles {
        let Some(edge) = ont.edge_types.iter_mut().find(|e| e.label == edge_profile.label) else {
            continue;
        };
        for stats in &edge_profile.property_stats {
            let Some(prop) = edge.properties.iter_mut().find(|p| p.name == stats.name) else {
                continue;
            };
            if let Some(enrichment) = format_enrichment(stats) {
                let old = prop.description.clone();
                let manual = strip_enrichment(&prop.description);
                let new_desc = match manual {
                    Some(manual) => format!("{manual}{ENRICHMENT_MARKER}{enrichment}"),
                    None => enrichment.clone(),
                };
                if prop.description.as_deref() != Some(&new_desc) {
                    changes.push(PropertyEnrichment {
                        entity_label: edge_profile.label.clone(),
                        entity_kind: "edge",
                        property_name: stats.name.clone(),
                        old_description: old,
                        new_description: new_desc.clone(),
                    });
                    prop.description = Some(new_desc);
                }
            }
        }
    }

    EnrichmentResult {
        ontology: ont,
        changes,
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Generate enrichment text from profiler stats.
/// Returns None if stats have no useful information to add.
fn format_enrichment(stats: &PropertyStats) -> Option<String> {
    if stats.distinct_count == 0 || stats.total_count == 0 {
        return None;
    }

    if stats.sample_values.is_empty() {
        // No samples available — try range
        match (&stats.min_value, &stats.max_value) {
            (Some(min), Some(max)) if min != max => Some(format!(
                "{PREFIX_RANGE}{min} ~ {max} ({} distinct)",
                stats.distinct_count
            )),
            _ => None,
        }
    } else if stats.distinct_count <= 30 {
        // Low-cardinality: show all values (enum-like)
        let quoted: Vec<String> = stats
            .sample_values
            .iter()
            .map(|v| format!("\"{v}\""))
            .collect();
        Some(format!(
            "{PREFIX_VALUES}{} ({} distinct)",
            quoted.join(", "),
            stats.distinct_count
        ))
    } else {
        // High-cardinality: show top examples for value format hint
        let quoted: Vec<String> = stats
            .sample_values
            .iter()
            .take(5)
            .map(|v| format!("\"{v}\""))
            .collect();
        Some(format!(
            "{PREFIX_EXAMPLES}{}, ... ({} distinct)",
            quoted.join(", "),
            stats.distinct_count
        ))
    }
}

/// Strip previous enrichment suffix from a description.
/// Returns the manual part only, or None if the entire description was generated.
///
/// Handles both the current `\n[enriched] ` marker and the legacy ` | ` separator
/// for seamless data migration on re-enrichment.
fn strip_enrichment(description: &Option<String>) -> Option<String> {
    let desc = description.as_deref()?;

    // Entire description is enrichment (no manual part)
    if desc.starts_with(PREFIX_VALUES)
        || desc.starts_with(PREFIX_RANGE)
        || desc.starts_with(PREFIX_EXAMPLES)
    {
        return None;
    }

    // Current format: newline + bracketed marker
    if let Some(idx) = desc.rfind(ENRICHMENT_MARKER) {
        let manual = desc[..idx].trim_end();
        return if manual.is_empty() {
            None
        } else {
            Some(manual.to_string())
        };
    }

    // Legacy format: ` | ` separator with enrichment prefix
    if let Some(idx) = desc.rfind(LEGACY_SEPARATOR) {
        let after = &desc[idx + LEGACY_SEPARATOR.len()..];
        if after.starts_with(PREFIX_VALUES)
            || after.starts_with(PREFIX_RANGE)
            || after.starts_with(PREFIX_EXAMPLES)
        {
            let manual = desc[..idx].trim_end();
            return if manual.is_empty() {
                None
            } else {
                Some(manual.to_string())
            };
        }
    }

    Some(desc.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stats(name: &str, samples: Vec<&str>, distinct: u64) -> PropertyStats {
        PropertyStats {
            name: name.to_string(),
            total_count: 100,
            null_count: 0,
            distinct_count: distinct,
            sample_values: samples.into_iter().map(String::from).collect(),
            min_value: None,
            max_value: None,
        }
    }

    fn make_range_stats(name: &str, min: &str, max: &str, distinct: u64) -> PropertyStats {
        PropertyStats {
            name: name.to_string(),
            total_count: 1000,
            null_count: 0,
            distinct_count: distinct,
            sample_values: vec![],
            min_value: Some(min.to_string()),
            max_value: Some(max.to_string()),
        }
    }

    #[test]
    fn format_low_cardinality() {
        let stats = make_stats("authority", vec!["EU_SCCS", "FDA", "MFDS"], 3);
        let result = format_enrichment(&stats).unwrap();
        assert!(result.starts_with("Values: "));
        assert!(result.contains("\"EU_SCCS\""));
        assert!(result.contains("(3 distinct)"));
    }

    #[test]
    fn format_range() {
        let stats = make_range_stats("price", "1000", "500000", 450);
        let result = format_enrichment(&stats).unwrap();
        assert!(result.starts_with("Range: "));
        assert!(result.contains("1000 ~ 500000"));
    }

    #[test]
    fn format_high_cardinality_examples() {
        let stats = PropertyStats {
            name: "product_name".into(),
            total_count: 5000,
            null_count: 0,
            distinct_count: 5000,
            sample_values: vec![
                "제품A".into(), "제품B".into(), "제품C".into(),
                "제품D".into(), "제품E".into(), "제품F".into(),
            ],
            min_value: Some("제품A".into()),
            max_value: Some("제품Z".into()),
        };
        let result = format_enrichment(&stats).unwrap();
        assert!(result.starts_with("Examples: "), "got: {result}");
        assert!(result.contains("\"제품A\""));
        assert!(result.contains("(5000 distinct)"));
        // Only top 5 values should be included
        assert!(!result.contains("\"제품F\""));
    }

    #[test]
    fn format_empty_stats() {
        let stats = PropertyStats {
            name: "x".into(),
            total_count: 0,
            null_count: 0,
            distinct_count: 0,
            sample_values: vec![],
            min_value: None,
            max_value: None,
        };
        assert!(format_enrichment(&stats).is_none());
    }

    #[test]
    fn strip_pure_enrichment() {
        let desc = Some("Values: \"a\", \"b\" (2 distinct)".to_string());
        assert_eq!(strip_enrichment(&desc), None);
    }

    #[test]
    fn strip_manual_plus_enrichment() {
        // Current format
        let desc = Some("Regulatory authority.\n[enriched] Values: \"EU_SCCS\" (1 distinct)".to_string());
        assert_eq!(
            strip_enrichment(&desc),
            Some("Regulatory authority.".to_string())
        );
    }

    #[test]
    fn strip_legacy_format() {
        // Legacy ` | ` separator still works for data migration
        let desc = Some("Regulatory authority. | Values: \"EU_SCCS\" (1 distinct)".to_string());
        assert_eq!(
            strip_enrichment(&desc),
            Some("Regulatory authority.".to_string())
        );
    }

    #[test]
    fn strip_preserves_pipe_in_manual() {
        // User description with pipes should NOT be stripped
        let desc = Some("Type A | Type B classification".to_string());
        assert_eq!(
            strip_enrichment(&desc),
            Some("Type A | Type B classification".to_string())
        );
    }

    #[test]
    fn strip_no_enrichment() {
        let desc = Some("A plain description".to_string());
        assert_eq!(
            strip_enrichment(&desc),
            Some("A plain description".to_string())
        );
    }

    #[test]
    fn strip_none() {
        assert_eq!(strip_enrichment(&None), None);
    }

    #[test]
    fn idempotent_enrichment() {
        // First enrichment
        let desc = Some("Manual desc".to_string());
        let manual = strip_enrichment(&desc);
        let enriched = format!("{}{ENRICHMENT_MARKER}Values: \"a\" (1 distinct)", manual.unwrap());
        // Second enrichment (should strip first, re-apply)
        let desc2 = Some(enriched);
        let manual2 = strip_enrichment(&desc2);
        assert_eq!(manual2, Some("Manual desc".to_string()));
    }
}

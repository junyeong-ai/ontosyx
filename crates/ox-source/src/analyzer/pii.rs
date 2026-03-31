use ox_core::source_analysis::{PiiDetectionMethod, PiiFinding, PiiType};
use ox_core::source_schema::{SourceProfile, SourceSchema};
use std::collections::HashSet;

pub(super) const PII_KEYWORDS: &[(&str, PiiType)] = &[
    ("email", PiiType::Email),
    // "mail" alone is ambiguous (email vs. physical mailing address) — flagged as PII
    // but without a specific type. The user clarifies intent during review.
    ("mail", PiiType::Other),
    ("phone", PiiType::Phone),
    ("mobile", PiiType::Phone),
    ("tel", PiiType::Phone),
    ("name", PiiType::Name),
    ("birth", PiiType::BirthDate),
    ("dob", PiiType::BirthDate),
    ("ssn", PiiType::NationalId),
    ("resident", PiiType::NationalId),
    ("rrn", PiiType::NationalId),
    ("address", PiiType::Address),
    ("addr", PiiType::Address),
    ("street", PiiType::Address),
    ("zip", PiiType::Address),
    ("postal", PiiType::Address),
];

pub(super) fn detect_pii(schema: &SourceSchema, profile: &SourceProfile) -> Vec<PiiFinding> {
    let mut findings = Vec::new();

    for table in &schema.tables {
        for col in &table.columns {
            let col_lower = col.name.to_lowercase();
            let segments: Vec<&str> = col_lower.split('_').collect();

            // Check column name keywords
            for (keyword, pii_type) in PII_KEYWORDS {
                if segments.contains(keyword) {
                    findings.push(PiiFinding {
                        table: table.name.clone(),
                        column: col.name.clone(),
                        pii_type: *pii_type,
                        detection_method: PiiDetectionMethod::ColumnName,
                        masked_preview: None,
                    });
                    break;
                }
            }
        }
    }

    // Pre-build a set of "table.column" keys found via name detection for O(1) skip check.
    let found_set: HashSet<String> = findings
        .iter()
        .map(|f| format!("{}.{}", f.table, f.column))
        .collect();

    // Value pattern: email detection (@ in sample values)
    for tp in &profile.table_profiles {
        for cs in &tp.column_stats {
            let key = format!("{}.{}", tp.table_name, cs.column_name);
            if found_set.contains(&key) {
                continue;
            }

            let has_email_value = cs
                .sample_values
                .iter()
                .any(|v| v.contains('@') && v.contains('.') && v.len() > 5);

            if has_email_value {
                let preview = cs
                    .sample_values
                    .iter()
                    .find(|v| v.contains('@'))
                    .map(|v| mask_email(v));

                findings.push(PiiFinding {
                    table: tp.table_name.clone(),
                    column: cs.column_name.clone(),
                    pii_type: PiiType::Email,
                    detection_method: PiiDetectionMethod::ValuePattern,
                    masked_preview: preview,
                });
            }
        }
    }

    findings
}

pub(super) fn mask_email(email: &str) -> String {
    let Some(at_pos) = email.find('@') else {
        return "***".to_string();
    };

    let local = &email[..at_pos]; // safe: '@' is ASCII, always a valid UTF-8 boundary
    let domain = &email[at_pos + 1..]; // safe: same reason

    // Use char-based length and slicing to handle Unicode local parts
    // (e.g., Korean internationalized email addresses).
    let masked_local = {
        let prefix: String = local.chars().take(2).collect();
        if local.chars().count() > 2 {
            format!("{prefix}**")
        } else {
            "**".to_string()
        }
    };

    // rfind('.') is byte-safe: '.' is ASCII, so dot_pos is always a valid UTF-8 boundary.
    let masked_domain = if let Some(dot_pos) = domain.rfind('.') {
        format!("***.{}", &domain[dot_pos + 1..])
    } else {
        "***".to_string()
    };

    format!("{masked_local}@{masked_domain}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::test_utils::make_schema;
    use ox_core::source_schema::{ColumnStats, TableProfile};

    #[test]
    fn mask_email_unicode_safe() {
        // Korean Unicode characters in local part must not panic
        let result = mask_email("홍길동@example.com");
        assert!(result.contains('@'), "must still contain @");
        assert!(result.ends_with(".com"), "domain TLD preserved");

        // Short local part (<=2 chars)
        let result = mask_email("ab@example.com");
        assert_eq!(result, "**@***.com");

        // Standard ASCII — regression guard
        let result = mask_email("hong@example.com");
        assert_eq!(result, "ho**@***.com");

        // No '@' at all — returns fallback
        assert_eq!(mask_email("notanemail"), "***");
    }

    #[test]
    fn detect_pii_false_positive_guard() {
        // `filename` — single token, not split by '_', must NOT match `name` keyword
        let schema = make_schema(&[("documents", &["id", "filename"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![],
        };
        let findings = detect_pii(&schema, &profile);
        assert!(
            findings.is_empty(),
            "`filename` must not match `name` keyword"
        );

        // `full_name` — splits to ["full", "name"] -> intentional PII detection (conservative)
        let schema2 = make_schema(&[("users", &["id", "full_name"])], &[]);
        let findings2 = detect_pii(&schema2, &profile);
        assert!(
            findings2.iter().any(|f| f.column == "full_name"),
            "`full_name` must be detected via `name` segment"
        );
    }

    #[test]
    fn detect_pii_column_name_keyword() {
        let schema = make_schema(
            &[("users", &["id", "email", "phone_number", "full_name"])],
            &[],
        );
        let profile = SourceProfile {
            table_profiles: vec![],
        };
        let findings = detect_pii(&schema, &profile);

        let cols: Vec<&str> = findings.iter().map(|f| f.column.as_str()).collect();
        assert!(cols.contains(&"email"), "email column must be detected");
        assert!(
            cols.contains(&"phone_number"),
            "phone_number must be detected"
        );
        assert!(cols.contains(&"full_name"), "full_name must be detected");
    }

    #[test]
    fn detect_pii_value_pattern_email() {
        // Column name gives no hint, but sample values contain email addresses
        let schema = make_schema(&[("contacts", &["id", "contact_info"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "contacts".to_string(),
                row_count: 2,
                column_stats: vec![ColumnStats {
                    column_name: "contact_info".to_string(),
                    null_count: 0,
                    distinct_count: 2,
                    sample_values: vec!["hong@example.com".to_string(), "kim@corp.io".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let findings = detect_pii(&schema, &profile);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].column, "contact_info");
        assert!(
            findings[0].masked_preview.is_some(),
            "must include masked preview"
        );
    }

    #[test]
    fn detect_pii_no_duplicate_for_column_name_and_value_pattern() {
        // email column found by name — value pattern must not add a second finding
        let schema = make_schema(&[("users", &["id", "email"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "users".to_string(),
                row_count: 1,
                column_stats: vec![ColumnStats {
                    column_name: "email".to_string(),
                    null_count: 0,
                    distinct_count: 1,
                    sample_values: vec!["hong@example.com".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let findings = detect_pii(&schema, &profile);
        let email_count = findings.iter().filter(|f| f.column == "email").count();
        assert_eq!(
            email_count, 1,
            "must not produce duplicate PII finding for same column"
        );
    }
}

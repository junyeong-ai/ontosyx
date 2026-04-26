//! `DataQualityDef` — first-class data-quality measure.
//!
//! The platform records quality as a separate concern from
//! business metrics. Where `MetricDef` answers "what is revenue?",
//! `DataQualityDef` answers "how much do we trust the number?".
//!
//! Five dimensions drawn from DAMA-DMBOK:
//!
//! - **Completeness** — fraction of rows with the expected field present.
//! - **Validity** — fraction of rows whose value matches the rule.
//! - **Uniqueness** — fraction of rows whose key is unique.
//! - **Consistency** — fraction of rows whose value agrees with a
//!   cross-reference.
//! - **Timeliness** — fraction of rows updated within the freshness
//!   window.
//! - **Accuracy** — reserved, requires a ground-truth comparator; the
//!   variant exists so forward-compatibility does not require a
//!   schema bump when the first accuracy measure lands.
//!
//! A `DataQualityDef` binds a dimension to a target (property or
//! node type) and either (a) references an existing `RuleDef` or
//! (b) names a source-dialect SQL assertion the scheduler runs.
//! Each run produces a `DataQualityMeasurement` record.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::ir::{NodeTypeId, PropertyId};
use crate::action::RuleId;

ox_core::define_id_newtype!(
    /// Stable identifier for a `DataQualityDef`.
    DataQualityId
);

/// Named data-quality measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct DataQualityDef {
    pub id: DataQualityId,
    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    pub target: DataQualityTarget,
    pub dimension: DataQualityDimensionKind,

    /// How the score is computed. Either a reference to a rule or an
    /// inline source-dialect assertion the scheduler evaluates.
    pub computation: DataQualityComputationKind,

    /// Score threshold in `[0, 1]`. A run whose score falls below
    /// `threshold` marks the measure as failing; the consumer
    /// (alert, dashboard, UI badge) decides what to do next.
    #[serde(default = "default_threshold")]
    pub threshold: f32,

    /// Most recent measurement, if any. Kept inline so a quick read
    /// of the def tells the caller how the last run went without a
    /// second round-trip to the measurement log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_measurement: Option<DataQualityMeasurement>,
}

fn default_threshold() -> f32 {
    0.95
}

/// What the measure is taken on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataQualityTarget {
    NodeType { node_type_id: NodeTypeId },
    Property {
        node_type_id: NodeTypeId,
        property_id: PropertyId,
    },
}

/// Dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataQualityDimensionKind {
    Completeness,
    Validity,
    Uniqueness,
    Consistency,
    Timeliness,
    /// Reserved — requires a ground-truth comparator. See module docs.
    Accuracy,
}

/// How the score is computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataQualityComputationKind {
    /// Delegate to a pre-defined rule. The rule's outcome across
    /// the population gives the score (pass / total).
    Rule { rule_id: RuleId },
    /// Inline SQL assertion. Must evaluate to a scalar `float`
    /// score in `[0, 1]`.
    SqlAssertion { query: String },
}

/// One run of a quality measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct DataQualityMeasurement {
    pub measured_at: DateTime<Utc>,
    /// Score in `[0, 1]`. 1.0 = perfect.
    pub score: f32,
    /// Number of entities inspected, for context in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<u64>,
    /// Free-form note — useful for human reviewers when the score
    /// drops and the dashboard wants to show the reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl DataQualityDef {
    /// Whether `last_measurement.score` clears `threshold`. Returns
    /// `None` when there is no measurement yet — a caller deciding
    /// what to render should treat that as "no data" rather than
    /// "passing" or "failing".
    pub fn is_passing(&self) -> Option<bool> {
        self.last_measurement.as_ref().map(|m| m.score >= self.threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_threshold_is_point95() {
        // Uses the default deserialised from a payload without an
        // explicit threshold — mirrors what the admin UI sends when
        // the author leaves the field blank.
        let payload = r#"{
            "id": "dq-1",
            "name": "customer_email_completeness",
            "target": { "kind": "property", "node_type_id": "nt-customer", "property_id": "prop-email" },
            "dimension": "completeness",
            "computation": { "kind": "sql_assertion", "query": "SELECT 1.0" }
        }"#;
        let dq: DataQualityDef = serde_json::from_str(payload).unwrap();
        assert_eq!(dq.threshold, 0.95);
    }

    #[test]
    fn is_passing_returns_none_without_measurement() {
        let dq = DataQualityDef {
            id: DataQualityId::new("dq-1"),
            name: "x".into(),
            description: LocalizedText::default(),
            target: DataQualityTarget::NodeType {
                node_type_id: NodeTypeId::new("nt-x"),
            },
            dimension: DataQualityDimensionKind::Completeness,
            computation: DataQualityComputationKind::Rule {
                rule_id: RuleId::new("r-1"),
            },
            threshold: 0.9,
            last_measurement: None,
        };
        assert_eq!(dq.is_passing(), None);
    }

    #[test]
    fn is_passing_compares_against_threshold() {
        let mut dq = DataQualityDef {
            id: DataQualityId::new("dq-1"),
            name: "x".into(),
            description: LocalizedText::default(),
            target: DataQualityTarget::NodeType {
                node_type_id: NodeTypeId::new("nt-x"),
            },
            dimension: DataQualityDimensionKind::Validity,
            computation: DataQualityComputationKind::SqlAssertion {
                query: "SELECT 1.0".into(),
            },
            threshold: 0.9,
            last_measurement: Some(DataQualityMeasurement {
                measured_at: Utc::now(),
                score: 0.95,
                sample_size: Some(1000),
                note: None,
            }),
        };
        assert_eq!(dq.is_passing(), Some(true));

        if let Some(m) = dq.last_measurement.as_mut() {
            m.score = 0.8;
        }
        assert_eq!(dq.is_passing(), Some(false));
    }
}

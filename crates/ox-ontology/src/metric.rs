//! `MetricDef` — first-class KPI / aggregate definition.
//!
//! A metric is a named aggregation that every downstream tool (BI
//! dashboards, alerting, LLM explanations) can reference without
//! re-deriving the SQL each time. Three roles it fills that loose
//! aggregate expressions did not:
//!
//! 1. **Named reference**: `metric_by_label("arr", …)` is stable;
//!    `SUM(subscription.amount_cents) / 100.0 * 12` is not.
//! 2. **Source-agnostic**: the `expression` carries dialect, but
//!    `unit`, `granularity`, and the `target_scope` are declared
//!    independently so the planner can emit Cypher or SQL from the
//!    same metric.
//! 3. **Temporal grain**: a metric declares whether it is a
//!    snapshot (current value) or a time series (monthly /
//!    weekly / daily). The planner materialises the series by
//!    re-evaluating the metric at each bucket boundary.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::ir::{EdgeTypeId, NodeTypeId};

ox_core::define_id_newtype!(
    /// Stable identifier for a `MetricDef`.
    MetricId
);

/// Named aggregate / KPI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MetricDef {
    pub id: MetricId,
    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    pub target_scope: MetricScope,

    /// Aggregation body. Dialect-tagged so the compiler can pick
    /// the right emitter at evaluation time.
    pub expression: MetricExpression,

    /// Unit of the result, rendered alongside the value in the UI.
    /// Free-form so a bilingual deployment can declare `"KRW"` or
    /// `"₩/월"` directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,

    /// Temporal shape of the metric.
    #[serde(default)]
    pub temporal_grain: TemporalGrain,
}

/// Which part of the ontology the metric aggregates over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetricScope {
    NodeType { node_type_id: NodeTypeId },
    EdgeType { edge_type_id: EdgeTypeId },
    /// Global metrics aggregate across the whole ontology snapshot.
    /// Typical: ontology completeness, total entity count, data
    /// quality rollups.
    Global,
}

/// Aggregation body in a specific dialect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetricExpression {
    SqlExpr { expression: String },
    CypherExpr { expression: String },
}

/// Snapshot / series choice.
///
/// A snapshot metric evaluates to a single value; a series metric
/// evaluates at `grain` intervals and produces `(t, value)` points.
/// The grain is inclusive: `Daily` → one point per day in the
/// queried window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalGrain {
    #[default]
    Snapshot,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_grain_is_snapshot() {
        assert_eq!(TemporalGrain::default(), TemporalGrain::Snapshot);
    }

    #[test]
    fn metric_roundtrips_through_json_with_optional_unit() {
        let m = MetricDef {
            id: MetricId::new("m-arr"),
            name: "ARR".into(),
            description: LocalizedText::default(),
            target_scope: MetricScope::NodeType {
                node_type_id: NodeTypeId::new("nt-subscription"),
            },
            expression: MetricExpression::SqlExpr {
                expression: "SUM(amount_cents) / 100.0 * 12".into(),
            },
            unit: Some("USD".into()),
            temporal_grain: TemporalGrain::Monthly,
        };
        let j = serde_json::to_value(&m).unwrap();
        let back: MetricDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn global_scope_serialises_as_tag_only() {
        let m = MetricDef {
            id: MetricId::new("m-count"),
            name: "total".into(),
            description: LocalizedText::default(),
            target_scope: MetricScope::Global,
            expression: MetricExpression::CypherExpr {
                expression: "MATCH (n) RETURN count(n)".into(),
            },
            unit: None,
            temporal_grain: TemporalGrain::Snapshot,
        };
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"kind\":\"global\""));
    }
}

//! Query-result visualization hints.
//!
//! `WidgetHint` is a lightweight recommendation attached to a query
//! execution result. It is distinct from `ox_ontology::widget_spec::WidgetSpec`:
//! the hint says which renderer family is likely appropriate for the result
//! shape, while `WidgetSpec` is a full dashboard authoring contract.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Simple hint about which widget family should render a query result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct WidgetHint {
    /// Which widget type to render.
    pub widget_type: WidgetType,
    /// Optional title for the widget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Brief reason for the selection. Useful for diagnostics, not
    /// intended as primary user-facing copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Available visualization widget families for query results.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    /// Categorical comparisons with a single metric.
    BarChart,
    /// Multiple metrics on the same category axis.
    ComboChart,
    /// Proportional distribution with few categories.
    PieChart,
    /// Time series or sequential trends.
    LineChart,
    /// Single aggregate value.
    StatCard,
    /// Multi-column detailed data.
    Table,
    /// Node-edge graph visualization.
    Graph,
    /// Matrix of values with color-coded intensity.
    Heatmap,
    /// Vertical event timeline.
    Timeline,
    /// Hierarchical area proportions.
    Treemap,
    /// Conversion or process funnel.
    Funnel,
    /// Data is self-explanatory from text alone.
    None,
}

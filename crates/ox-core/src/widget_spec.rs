use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// WidgetSpec — frontend-agnostic visualization specification
//
// Describes WHAT to render (data shape, visualization type, interactivity)
// without binding to any specific frontend framework.
//
// The frontend (Next.js / React) receives WidgetSpec as JSON and renders
// the appropriate component (TanStack Table, react-force-graph, Recharts).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "widget", rename_all = "snake_case")]
pub enum WidgetSpec {
    /// Tabular data display
    Table(TableSpec),
    /// Node-edge graph visualization
    Graph(GraphSpec),
    /// Statistical chart
    Chart(ChartSpec),
    /// Rich text / markdown
    Text(TextSpec),
    /// Multiple widgets in a layout
    Composite(CompositeSpec),
    /// Code block (for !ox query results, Cypher preview, etc.)
    Code(CodeSpec),
}

// ---------------------------------------------------------------------------
// TableSpec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableSpec {
    /// Column definitions
    pub columns: Vec<WidgetColumnDef>,
    /// Whether columns are sortable
    pub sortable: bool,
    /// Whether columns are filterable
    pub filterable: bool,
    /// Rows per page (0 = show all)
    pub page_size: usize,
    /// Whether to show an export button (CSV/JSON)
    pub export_enabled: bool,
    /// Optional title
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WidgetColumnDef {
    /// Column key (maps to data field)
    pub key: String,
    /// Display label
    pub label: String,
    /// Data type hint for formatting
    pub data_type: ColumnDataType,
    /// Optional fixed width in pixels
    pub width: Option<u32>,
    /// Text alignment
    pub align: ColumnAlign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnDataType {
    Text,
    Number,
    Date,
    DateTime,
    Boolean,
    Link,
    Badge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnAlign {
    Left,
    Center,
    Right,
}

// ---------------------------------------------------------------------------
// GraphSpec — interactive graph visualization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphSpec {
    /// How to render nodes
    pub node_config: NodeVizConfig,
    /// How to render edges
    pub edge_config: EdgeVizConfig,
    /// Layout algorithm
    pub layout: GraphLayout,
    /// Whether the graph is interactive (drag, zoom, click)
    pub interactive: bool,
    /// Whether to show zoom controls
    pub zoom_enabled: bool,
    /// Optional title
    pub title: Option<String>,
    /// Max nodes to render (performance guard)
    pub max_nodes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeVizConfig {
    /// Which data field to use as node label
    pub label_field: String,
    /// Which data field determines node color (optional)
    pub color_field: Option<String>,
    /// Color mapping: field value → hex color
    pub color_map: Option<Vec<ColorMapping>>,
    /// Which data field determines node size (optional)
    pub size_field: Option<String>,
    /// Fields to show in tooltip on hover
    pub tooltip_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EdgeVizConfig {
    /// Which data field to use as edge label
    pub label_field: Option<String>,
    /// Which data field determines edge color
    pub color_field: Option<String>,
    /// Which data field determines edge thickness
    pub weight_field: Option<String>,
    /// Whether to show arrows
    pub directed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ColorMapping {
    pub value: String,
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphLayout {
    /// Force-directed (d3-force / react-force-graph)
    Force,
    /// Top-down hierarchical
    Hierarchical,
    /// Radial/circular
    Radial,
    /// Dagre (directed acyclic graph)
    Dagre,
}

// ---------------------------------------------------------------------------
// ChartSpec — statistical visualization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChartSpec {
    /// Chart type
    pub chart_type: ChartType,
    /// X axis configuration
    pub x_axis: AxisSpec,
    /// Y axis configuration
    pub y_axis: AxisSpec,
    /// Data series
    pub series: Vec<SeriesSpec>,
    /// Whether to show legend
    pub show_legend: bool,
    /// Optional title
    pub title: Option<String>,
    /// Whether the chart is interactive (hover tooltips, click)
    pub interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
    Heatmap,
    Treemap,
    Radar,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AxisSpec {
    /// Data field for this axis
    pub field: String,
    /// Display label
    pub label: Option<String>,
    /// Scale type
    pub scale: AxisScale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxisScale {
    Linear,
    Log,
    Time,
    Category,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeriesSpec {
    /// Data field for series values
    pub field: String,
    /// Display label
    pub label: String,
    /// Optional fixed color (hex)
    pub color: Option<String>,
}

// ---------------------------------------------------------------------------
// TextSpec — rich text display
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextSpec {
    /// The text content
    pub content: String,
    /// Format of the content
    pub format: TextFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    Plain,
    Markdown,
    Html,
}

// ---------------------------------------------------------------------------
// CodeSpec — code/query display
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeSpec {
    /// The code content
    pub content: String,
    /// Language for syntax highlighting
    pub language: String,
    /// Optional title
    pub title: Option<String>,
    /// Whether to show copy button
    pub copyable: bool,
}

// ---------------------------------------------------------------------------
// CompositeSpec — multiple widgets in a layout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompositeSpec {
    /// Layout direction
    pub layout: WidgetLayout,
    /// Child widgets
    pub children: Vec<WidgetSpec>,
    /// Optional title for the composite
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WidgetLayout {
    /// Horizontal row
    Row,
    /// Vertical column
    Column,
    /// CSS grid with specified columns
    Grid { cols: u32 },
}

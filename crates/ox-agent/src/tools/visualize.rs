use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use ox_brain::WidgetType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// VisualizeTool — generate chart/visualization specifications
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VisualizeInput {
    /// Description of the desired visualization.
    pub description: String,
    /// Chart type to render. Choose the most appropriate type for the data:
    /// bar_chart, line_chart, pie_chart, combo_chart, stat_card, table, graph.
    pub chart_type: WidgetType,
    /// Data to visualize (JSON array of objects).
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct VisualizeOutput {
    chart_type: WidgetType,
    title: String,
    columns: Vec<String>,
    row_count: usize,
    /// Actual data rows for chart rendering.
    data: serde_json::Value,
}

/// Generates chart/visualization specifications from data.
/// The LLM selects the appropriate chart type based on data shape and context.
/// Returns a widget spec that the frontend renders.
pub struct VisualizeTool;

#[async_trait]
impl SchemaTool for VisualizeTool {
    type Input = VisualizeInput;
    const NAME: &'static str = super::VISUALIZE;
    const DESCRIPTION: &'static str =
        "Generate a chart or visualization specification from data. \
         Choose the chart_type that best represents the data: \
         bar_chart (categorical comparisons), line_chart (time series/trends), \
         pie_chart (proportions, ≤8 segments), combo_chart (multiple metrics on same axis), \
         stat_card (single key metric), table (detailed tabular data), \
         graph (relationship/network visualization). \
         Returns a spec the frontend renders.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let columns = if let Some(arr) = input.data.as_array() {
            arr.first()
                .and_then(|row| row.as_object())
                .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            vec![]
        };

        let row_count = input.data.as_array().map(|a| a.len()).unwrap_or(0);

        let output = VisualizeOutput {
            chart_type: input.chart_type,
            title: input.description,
            columns,
            row_count,
            data: input.data,
        };

        ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
    }
}

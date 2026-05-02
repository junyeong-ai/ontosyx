export const WIDGET_TYPES = [
  { value: "table", label: "Table" },
  { value: "bar_chart", label: "Bar Chart" },
  { value: "line_chart", label: "Line Chart" },
  { value: "pie_chart", label: "Pie Chart" },
  { value: "combo_chart", label: "Combo Chart" },
  { value: "stat_card", label: "Stat Card" },
  { value: "graph", label: "Graph" },
  { value: "heatmap", label: "Heatmap" },
  { value: "timeline", label: "Timeline" },
  { value: "treemap", label: "Treemap" },
  { value: "funnel", label: "Funnel" },
  { value: "scatter", label: "Scatter Plot" },
  { value: "histogram", label: "Histogram" },
] as const;

export type WidgetTypeName = (typeof WIDGET_TYPES)[number]["value"];

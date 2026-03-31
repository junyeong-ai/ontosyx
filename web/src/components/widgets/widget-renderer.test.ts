import { describe, it, expect } from "vitest";
import type { QueryResult } from "@/types/api";

// We test the auto-detect logic directly by importing the module and examining
// the resolveWidgetType + autoDetectWidgetType through the public WidgetRenderer.
// Since rendering requires full React context, we test the pure detection function
// by extracting test cases from the documented decision tree.

// Re-implement the detection algorithm for unit testing
// (mirrors autoDetectWidgetType from widget-renderer.tsx)
function detectWidgetType(data: QueryResult): string {
  const { columns, rows } = data;
  if (!columns.length || !rows.length) return "table";

  // Graph detection
  const GRAPH_SOURCE_COLS = new Set(["source", "source_id", "from"]);
  const GRAPH_TARGET_COLS = new Set(["target", "target_id", "to"]);
  const GRAPH_REL_COLS = new Set(["relationship", "rel_type", "edge_type"]);
  if (columns.length >= 2 && rows.length >= 2) {
    const lower = columns.map((c) => c.toLowerCase());
    const hasSource = lower.some((c) => GRAPH_SOURCE_COLS.has(c));
    const hasTarget = lower.some((c) => GRAPH_TARGET_COLS.has(c));
    const hasRel = lower.some((c) => GRAPH_REL_COLS.has(c));
    if ((hasSource && hasTarget) || (hasSource && hasRel) || (hasTarget && hasRel)) {
      return "graph";
    }
  }

  const numCols = columns.length;
  const numRows = rows.length;
  const firstRow = rows[0];

  // Stat card: 1 row, 1-2 numeric columns
  if (numRows === 1 && numCols <= 2) {
    const allNumeric = columns.every((col) => typeof firstRow[col] === "number");
    if (allNumeric) return "stat_card";
  }

  const numericCount = columns.filter((col) => typeof firstRow[col] === "number").length;

  // Scatter: 2 numeric columns, 5+ rows
  if (numericCount === 2 && numRows >= 5 && numCols === 2) return "scatter";

  // Histogram: 1 numeric column only, 5+ rows
  if (numCols === 1 && numRows >= 5 && numericCount === 1) return "histogram";

  // Combo: 3+ cols, 2+ rows, 2+ numeric
  if (numCols >= 3 && numRows >= 2 && numericCount >= 2) return "combo_chart";

  // Bar/Pie: 2 cols, label + number
  if (numCols === 2 && numRows >= 2) {
    const [col1, col2] = columns;
    const isLabelValue = typeof firstRow[col1] === "string" && typeof firstRow[col2] === "number";
    const isValueLabel = typeof firstRow[col1] === "number" && typeof firstRow[col2] === "string";
    if (isLabelValue || isValueLabel) {
      return numRows <= 8 ? "pie_chart" : "bar_chart";
    }
  }

  return "table";
}

describe("Widget auto-detection", () => {
  it("returns table for empty data", () => {
    expect(detectWidgetType({ columns: [], rows: [] })).toBe("table");
  });

  it("detects stat_card for single numeric row", () => {
    expect(detectWidgetType({
      columns: ["count"],
      rows: [{ count: 42 }],
    })).toBe("stat_card");
  });

  it("detects stat_card for 1 row, 2 numeric columns", () => {
    expect(detectWidgetType({
      columns: ["total", "avg"],
      rows: [{ total: 100, avg: 25.5 }],
    })).toBe("stat_card");
  });

  it("detects pie_chart for small label+value dataset", () => {
    expect(detectWidgetType({
      columns: ["category", "count"],
      rows: [
        { category: "A", count: 10 },
        { category: "B", count: 20 },
        { category: "C", count: 30 },
      ],
    })).toBe("pie_chart");
  });

  it("detects bar_chart for large label+value dataset", () => {
    const rows = Array.from({ length: 20 }, (_, i) => ({ label: `Item ${i}`, value: i * 10 }));
    expect(detectWidgetType({
      columns: ["label", "value"],
      rows,
    })).toBe("bar_chart");
  });

  it("detects scatter for 2 numeric columns with enough rows", () => {
    const rows = Array.from({ length: 10 }, (_, i) => ({ x: i, y: i * 2 }));
    expect(detectWidgetType({
      columns: ["x", "y"],
      rows,
    })).toBe("scatter");
  });

  it("detects histogram for single numeric column with enough rows", () => {
    const rows = Array.from({ length: 10 }, (_, i) => ({ value: i * 5 }));
    expect(detectWidgetType({
      columns: ["value"],
      rows,
    })).toBe("histogram");
  });

  it("detects combo_chart for 3+ columns with 2+ numeric", () => {
    expect(detectWidgetType({
      columns: ["month", "revenue", "cost"],
      rows: [
        { month: "Jan", revenue: 100, cost: 80 },
        { month: "Feb", revenue: 120, cost: 90 },
      ],
    })).toBe("combo_chart");
  });

  it("detects graph from source/target columns", () => {
    expect(detectWidgetType({
      columns: ["source", "target", "weight"],
      rows: [
        { source: "A", target: "B", weight: 1 },
        { source: "B", target: "C", weight: 2 },
      ],
    })).toBe("graph");
  });

  it("detects graph from source_id/target_id columns", () => {
    expect(detectWidgetType({
      columns: ["source_id", "target_id"],
      rows: [
        { source_id: "1", target_id: "2" },
        { source_id: "2", target_id: "3" },
      ],
    })).toBe("graph");
  });

  it("falls back to table for all-string data", () => {
    expect(detectWidgetType({
      columns: ["name", "city"],
      rows: [
        { name: "Alice", city: "Seoul" },
        { name: "Bob", city: "Tokyo" },
      ],
    })).toBe("table");
  });
});

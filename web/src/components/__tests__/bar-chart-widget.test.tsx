import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../messages/en.json";
import { BarChartWidget } from "@/components/widgets/bar-chart-widget";
import type { QueryResult, WidgetSpec } from "@/types/ontology";

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

// Mock recharts — jsdom can't lay out SVG, we only care that the data flow works.
vi.mock("recharts", async () => {
  const React = await import("react");
  return {
    BarChart: ({ children, data }: { children: React.ReactNode; data: unknown[] }) =>
      React.createElement(
        "div",
        { "data-testid": "mock-bar-chart", "data-rows": data.length },
        children,
      ),
    Bar: () => React.createElement("div", { "data-testid": "mock-bar" }),
    XAxis: () => React.createElement("div", { "data-testid": "mock-xaxis" }),
    YAxis: () => React.createElement("div", { "data-testid": "mock-yaxis" }),
    Tooltip: () => React.createElement("div", { "data-testid": "mock-tooltip" }),
    CartesianGrid: () => React.createElement("div", { "data-testid": "mock-grid" }),
    ResponsiveContainer: ({ children }: { children: React.ReactNode }) =>
      React.createElement("div", { "data-testid": "mock-container" }, children),
  };
});

vi.mock("@/lib/use-dark-mode", () => ({
  useIsDarkMode: () => false,
}));

describe("BarChartWidget", () => {
  it("renders data with valid label + value columns", () => {
    const spec: WidgetSpec = { widget_type: "bar_chart", title: "카테고리 매출" };
    const data: QueryResult = {
      columns: ["category", "revenue"],
      rows: [
        { category: "전자기기", revenue: 5078000 },
        { category: "패션", revenue: 813000 },
        { category: "식품", revenue: 280000 },
      ],
    };
    renderWithIntl(<BarChartWidget spec={spec} data={data} />);
    expect(screen.getByText("카테고리 매출")).toBeInTheDocument();
    expect(screen.getByTestId("mock-bar-chart")).toHaveAttribute(
      "data-rows",
      "3",
    );
    expect(screen.getByText("3 items")).toBeInTheDocument();
  });

  it("shows insufficient-columns message when data is empty", () => {
    const spec: WidgetSpec = { widget_type: "bar_chart" };
    const data: QueryResult = { columns: [], rows: [] };
    renderWithIntl(<BarChartWidget spec={spec} data={data} />);
    expect(
      screen.getByText(/insufficient columns for chart/i),
    ).toBeInTheDocument();
  });

  it("rotates axis labels when category count exceeds threshold", () => {
    const spec: WidgetSpec = { widget_type: "bar_chart" };
    const rows = Array.from({ length: 12 }, (_, i) => ({
      name: `item-${i}`,
      value: i * 10,
    }));
    const data: QueryResult = { columns: ["name", "value"], rows };
    renderWithIntl(<BarChartWidget spec={spec} data={data} />);
    expect(screen.getByText("12 items")).toBeInTheDocument();
  });
});

/**
 * A11y test: chart widgets expose role/aria-label on the key SVG wrapper.
 *
 * We mock Recharts (jsdom can't lay out SVG) and assert the *wrapper*
 * div carries `role="img"` + a descriptive `aria-label`, which is what
 * screen readers actually speak.
 */
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { axe } from "vitest-axe";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../../messages/en.json";
import { BarChartWidget } from "@/components/dashboard/widgets/bar-chart-widget";
import type { QueryResult, WidgetSpec } from "@/types/api";

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

afterEach(cleanup);

vi.mock("recharts", async () => {
  const React = await import("react");
  return {
    BarChart: ({ children }: { children: React.ReactNode }) =>
      React.createElement("div", { "data-testid": "mock-bar-chart" }, children),
    Bar: () => React.createElement("div", { "data-testid": "mock-bar" }),
    XAxis: () => React.createElement("div", { "data-testid": "mock-xaxis" }),
    YAxis: () => React.createElement("div", { "data-testid": "mock-yaxis" }),
    Tooltip: () => React.createElement("div", { "data-testid": "mock-tooltip" }),
    CartesianGrid: () => React.createElement("div", { "data-testid": "mock-grid" }),
    ResponsiveContainer: ({ children }: { children: React.ReactNode }) =>
      React.createElement("div", { "data-testid": "mock-container" }, children),
  };
});

vi.mock("@/hooks/use-dark-mode", () => ({ useIsDarkMode: () => false }));

describe("BarChartWidget (a11y)", () => {
  const spec: WidgetSpec = { widget_type: "bar_chart", title: "Revenue" };
  const data: QueryResult = {
    columns: ["category", "revenue"],
    rows: [
      { category: "A", revenue: 10 },
      { category: "B", revenue: 20 },
    ],
  };

  it("exposes role=img with a descriptive aria-label", () => {
    const { container } = renderWithIntl(<BarChartWidget spec={spec} data={data} />);
    const wrapper = container.querySelector('[role="img"]');
    expect(wrapper).not.toBeNull();
    expect(wrapper?.getAttribute("aria-label")).toMatch(/bar chart/i);
    expect(wrapper?.getAttribute("aria-label")).toMatch(/Revenue/);
  });

  it("has no axe violations", async () => {
    const { container } = renderWithIntl(<BarChartWidget spec={spec} data={data} />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});

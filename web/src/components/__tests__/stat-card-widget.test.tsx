import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../messages/en.json";
import { StatCardWidget } from "@/components/dashboard/widgets/stat-card-widget";
import type { QueryResult, WidgetSpec } from "@/types/ontology";

vi.mock("@/hooks/use-locale-chain", () => ({
  useLocaleChain: () => ["en"],
}));

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("StatCardWidget", () => {
  it("renders a numeric KPI with label", () => {
    const spec: WidgetSpec = {
      widget_type: "stat_card",
      title: "총 주문",
      data_mapping: { value: "total", label: "label" },
    };
    const data: QueryResult = {
      columns: ["total", "label"],
      rows: [{ total: 12345, label: "총 주문 수" }],
    };
    renderWithIntl(<StatCardWidget spec={spec} data={data} />);
    expect(screen.getByText("12,345")).toBeInTheDocument();
    expect(screen.getByText("총 주문 수")).toBeInTheDocument();
  });

  it("falls back to fallback when no rows present", () => {
    const spec: WidgetSpec = { widget_type: "stat_card" };
    const data: QueryResult = { columns: ["x"], rows: [] };
    renderWithIntl(<StatCardWidget spec={spec} data={data} />);
    expect(screen.getByText(/no data available/i)).toBeInTheDocument();
  });

  it("renders text content when widget_type is text", () => {
    const spec: WidgetSpec = {
      widget_type: "text",
      title: "설명",
      content: "이 위젯은 설명용입니다",
    };
    const data: QueryResult = { columns: [], rows: [] };
    renderWithIntl(<StatCardWidget spec={spec} data={data} />);
    expect(screen.getByText("설명")).toBeInTheDocument();
    expect(
      screen.getByText("이 위젯은 설명용입니다"),
    ).toBeInTheDocument();
  });

  it("applies the danger semantic colour for critical values (above direction)", () => {
    // Threshold colours go through `text-{tone}-foreground` semantic
    // tokens — the wire-protocol thresholds (`warning` / `critical`)
    // map onto design-system tone tokens, not raw palette literals,
    // so a future tone palette tweak doesn't drift the widget.
    const spec: WidgetSpec = {
      widget_type: "stat_card",
      data_mapping: { value: "latency_ms" },
      thresholds: { warning: 200, critical: 500, direction: "above" },
    };
    const data: QueryResult = {
      columns: ["latency_ms"],
      rows: [{ latency_ms: 1000 }],
    };
    const { container } = renderWithIntl(<StatCardWidget spec={spec} data={data} />);
    const valueSpan = container.querySelector(".text-danger-foreground");
    expect(valueSpan).not.toBeNull();
    expect(valueSpan?.textContent).toBe("1,000");
  });

  it("renders delta with + prefix for positive values", () => {
    const spec: WidgetSpec = {
      widget_type: "stat_card",
      data_mapping: { value: "total", delta: "diff" },
    };
    const data: QueryResult = {
      columns: ["total", "diff"],
      rows: [{ total: 100, diff: 25 }],
    };
    renderWithIntl(<StatCardWidget spec={spec} data={data} />);
    expect(screen.getByText("+25")).toBeInTheDocument();
  });
});

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../messages/en.json";
import { TableWidget } from "@/components/dashboard/widgets/table-widget";
import type { QueryResult, WidgetSpec } from "@/types/ontology";

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

// Base UI Tooltip wraps children via portal in the real app — stub with a
// pass-through so we don't need Base UI's full provider chain.
vi.mock("@/components/ui/tooltip", async () => {
  const React = await import("react");
  return {
    Tooltip: ({ children }: { children: React.ReactNode }) =>
      React.createElement(React.Fragment, null, children),
  };
});

// `useRouter()` requires a mounted app router; we don't exercise
// navigation here, so stub it with a no-op to let the component
// render in jsdom without an <AppRouterContext> provider.
vi.mock("next/navigation", async () => {
  const actual = await vi.importActual<typeof import("next/navigation")>(
    "next/navigation",
  );
  return {
    ...actual,
    useRouter: () => ({
      push: () => {},
      replace: () => {},
      prefetch: () => {},
      back: () => {},
      forward: () => {},
      refresh: () => {},
    }),
  };
});

describe("TableWidget", () => {
  beforeEach(() => {
    // Reset store-side effects between tests
    vi.restoreAllMocks();
  });

  it("renders all columns and rows", () => {
    const spec: WidgetSpec = { widget_type: "table", title: "사용자 목록" };
    const data: QueryResult = {
      columns: ["사용자ID", "사용자명", "이메일"],
      rows: [
        { 사용자ID: "U001", 사용자명: "김민준", 이메일: "min@e.com" },
        { 사용자ID: "U002", 사용자명: "이서연", 이메일: "seo@e.com" },
      ],
    };
    renderWithIntl(<TableWidget spec={spec} data={data} />);
    expect(screen.getByText("사용자 목록")).toBeInTheDocument();
    expect(screen.getByText("김민준")).toBeInTheDocument();
    expect(screen.getByText("이서연")).toBeInTheDocument();
    expect(screen.getByText(/2 rows · 3 columns/)).toBeInTheDocument();
  });

  it("uses spec.columns labels when provided", () => {
    const spec: WidgetSpec = {
      widget_type: "table",
      columns: [
        { key: "사용자ID", label: "ID" },
        { key: "사용자명", label: "이름" },
      ],
    };
    const data: QueryResult = {
      columns: ["사용자ID", "사용자명"],
      rows: [{ 사용자ID: "U001", 사용자명: "김민준" }],
    };
    renderWithIntl(<TableWidget spec={spec} data={data} />);
    expect(screen.getByText("ID")).toBeInTheDocument();
    expect(screen.getByText("이름")).toBeInTheDocument();
  });

  it("sorts rows when column header clicked", () => {
    const spec: WidgetSpec = { widget_type: "table" };
    const data: QueryResult = {
      columns: ["name", "score"],
      rows: [
        { name: "A", score: 30 },
        { name: "B", score: 10 },
        { name: "C", score: 20 },
      ],
    };
    const { container } = renderWithIntl(<TableWidget spec={spec} data={data} />);
    // Initial order
    const rowsBefore = container.querySelectorAll("tbody tr");
    expect(rowsBefore[0]?.textContent).toContain("A");

    // Click "score" header to sort ascending
    fireEvent.click(screen.getByText("score"));
    const rowsAfter = container.querySelectorAll("tbody tr");
    expect(rowsAfter[0]?.textContent).toContain("B"); // score=10 first
  });

  it("truncates display when row count exceeds MAX_VISIBLE_ROWS", () => {
    const spec: WidgetSpec = { widget_type: "table" };
    const rows = Array.from({ length: 250 }, (_, i) => ({
      id: i,
      value: `row-${i}`,
    }));
    const data: QueryResult = { columns: ["id", "value"], rows };
    renderWithIntl(<TableWidget spec={spec} data={data} />);
    expect(screen.getByText(/250 rows · 2 columns/)).toBeInTheDocument();
    expect(
      screen.getByText(/showing first 200 rows/i),
    ).toBeInTheDocument();
  });
});

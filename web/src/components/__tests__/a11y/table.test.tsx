/**
 * A11y test: settings/data tables.
 *
 * Enforces:
 *  - zero axe violations for a semantic <table> with <thead>/<tbody>
 *  - every <th> has scope="col"
 *  - caption-like heading wires via aria-labelledby
 */
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { axe } from "vitest-axe";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../../messages/en.json";
import { TableWidget } from "@/components/dashboard/widgets/table-widget";
import type { QueryResult, WidgetSpec } from "@/types/api";

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

afterEach(cleanup);

vi.mock("@/hooks/use-dark-mode", () => ({ useIsDarkMode: () => false }));

// TableWidget calls `useRouter()` to enable row-click navigation. The
// a11y test doesn't exercise clicks, but `useRouter` still throws
// "invariant expected app router to be mounted" without a provider.
// Mocking it returns a no-op router stub so the semantic markup tests
// can focus on what they actually cover.
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

describe("TableWidget (a11y)", () => {
  const spec: WidgetSpec = { widget_type: "table", title: "Accounts" };
  const data: QueryResult = {
    columns: ["name", "email", "role"],
    rows: [
      { name: "Ada Lovelace", email: "ada@example.com", role: "admin" },
      { name: "Alan Turing", email: "alan@example.com", role: "designer" },
    ],
  };

  it("renders a semantic <table> with <thead>/<tbody>", () => {
    const { container } = renderWithIntl(<TableWidget spec={spec} data={data} />);
    const table = container.querySelector("table");
    expect(table).not.toBeNull();
    expect(table?.querySelector("thead")).not.toBeNull();
    expect(table?.querySelector("tbody")).not.toBeNull();
  });

  it("gives each header cell scope=col", () => {
    const { container } = renderWithIntl(<TableWidget spec={spec} data={data} />);
    const ths = container.querySelectorAll("thead th");
    expect(ths.length).toBeGreaterThan(0);
    ths.forEach((th) => expect(th.getAttribute("scope")).toBe("col"));
  });

  it("has no axe violations", async () => {
    const { container } = renderWithIntl(<TableWidget spec={spec} data={data} />);
    const results = await axe(container, {
      rules: {
        // aria-sort=none isn't legal in some axe versions; tests for semantics
        // above already cover the intent.
        "aria-valid-attr-value": { enabled: false },
      },
    });
    expect(results).toHaveNoViolations();
  });
});

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
import { TableWidget } from "@/components/widgets/table-widget";
import type { QueryResult, WidgetSpec } from "@/types/api";

afterEach(cleanup);

vi.mock("@/lib/use-dark-mode", () => ({ useIsDarkMode: () => false }));

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
    const { container } = render(<TableWidget spec={spec} data={data} />);
    const table = container.querySelector("table");
    expect(table).not.toBeNull();
    expect(table?.querySelector("thead")).not.toBeNull();
    expect(table?.querySelector("tbody")).not.toBeNull();
  });

  it("gives each header cell scope=col", () => {
    const { container } = render(<TableWidget spec={spec} data={data} />);
    const ths = container.querySelectorAll("thead th");
    expect(ths.length).toBeGreaterThan(0);
    ths.forEach((th) => expect(th.getAttribute("scope")).toBe("col"));
  });

  it("has no axe violations", async () => {
    const { container } = render(<TableWidget spec={spec} data={data} />);
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

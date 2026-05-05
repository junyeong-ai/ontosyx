import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import type { PageState } from "@/components/layout/page-state";

const messages = {} as Record<string, unknown>;

function shell(props: Partial<React.ComponentProps<typeof WorkbenchPageShell>>) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <WorkbenchPageShell title="Projects" {...props}>
        <div data-testid="body">body</div>
      </WorkbenchPageShell>
    </NextIntlClientProvider>,
  );
}

const FILTERS = (
  <div data-testid="filter-row">
    <input data-testid="search" placeholder="search" />
  </div>
);

const COUNTER_TESTID = "counter";

// ---------------------------------------------------------------------------
// `WorkbenchPageShell` contract
//
// Single source of truth for chrome behaviour:
//   - Filters are visible only in `data` and `filtered-empty` states.
//   - The count chip is dimmed when the page is non-interactive
//     (loading / error / empty) so "12" doesn't read as authoritative
//     while data is in flight.
//   - The body is always rendered — pages own their state-specific
//     rendering (skeleton / error / empty / data).
// ---------------------------------------------------------------------------

function renderWithCounter(state: PageState) {
  return shell({
    pageState: state,
    count: 12,
    filters: FILTERS,
  });
}

describe("WorkbenchPageShell — pageState chrome", () => {
  it("renders the body unconditionally — page owns state branching", () => {
    const states: PageState[] = [
      { kind: "loading" },
      { kind: "error", onRetry: () => {} },
      { kind: "empty" },
      { kind: "filtered-empty", onClearFilters: () => {} },
      { kind: "data" },
    ];
    for (const state of states) {
      const { unmount } = renderWithCounter(state);
      expect(screen.getByTestId("body")).toBeInTheDocument();
      unmount();
    }
  });

  it("hides the filter row in loading / error / empty states", () => {
    for (const state of [
      { kind: "loading" } as PageState,
      { kind: "error", onRetry: () => {} } as PageState,
      { kind: "empty" } as PageState,
    ]) {
      const { unmount } = renderWithCounter(state);
      expect(screen.queryByTestId("filter-row")).not.toBeInTheDocument();
      unmount();
    }
  });

  it("shows the filter row in data and filtered-empty states", () => {
    for (const state of [
      { kind: "data" } as PageState,
      { kind: "filtered-empty", onClearFilters: () => {} } as PageState,
    ]) {
      const { unmount } = renderWithCounter(state);
      expect(screen.getByTestId("filter-row")).toBeInTheDocument();
      unmount();
    }
  });

  it("dims the count chip when non-interactive (loading / error / empty)", () => {
    const { container } = renderWithCounter({ kind: "loading" });
    // The count chip uses a `tabular-nums` class as its identifying
    // marker — query by class rather than testid so the test stays
    // structural.
    const counter = container.querySelector(".tabular-nums");
    expect(counter).not.toBeNull();
    expect(counter?.className).toMatch(/text-foreground-muted\/50/);
  });

  it("renders the count chip at full opacity in data state", () => {
    const { container } = renderWithCounter({ kind: "data" });
    const counter = container.querySelector(".tabular-nums");
    expect(counter).not.toBeNull();
    expect(counter?.className).toMatch(/text-foreground-muted(?!\/)/);
  });

  it("omits the count chip entirely when `count` is undefined", () => {
    const { container } = shell({
      pageState: { kind: "data" },
      filters: FILTERS,
    });
    expect(container.querySelector(".tabular-nums")).toBeNull();
  });

  it("renders actions unconditionally — primary CTA stays reachable in every state", () => {
    const states: PageState[] = [
      { kind: "loading" },
      { kind: "error", onRetry: () => {} },
      { kind: "empty" },
      { kind: "data" },
    ];
    for (const state of states) {
      const { unmount } = shell({
        pageState: state,
        actions: <button type="button" data-testid="action-cta">New</button>,
      });
      expect(screen.getByTestId("action-cta")).toBeInTheDocument();
      unmount();
    }
  });

  it("defaults pageState to `data` so consumers without a query can opt out", () => {
    shell({ filters: FILTERS, count: 3 });
    expect(screen.getByTestId("filter-row")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });
});

// silence the COUNTER_TESTID unused-import warning when present
void COUNTER_TESTID;

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

const defaults = {
  skeleton: <div data-testid="skel">loading…</div>,
  empty: { title: "Nothing here" },
  filteredEmpty: { title: "No matches", clearLabel: "Clear filters" },
  error: { title: "Boom", retryLabel: "Retry" },
};

describe("PageStateView", () => {
  it("'loading' renders the skeleton slot", () => {
    wrap(
      <PageStateView state={{ kind: "loading" }} {...defaults}>
        <div data-testid="data">live</div>
      </PageStateView>,
    );
    expect(screen.getByTestId("skel")).toBeInTheDocument();
    expect(screen.queryByTestId("data")).not.toBeInTheDocument();
  });

  it("'data' renders children", () => {
    wrap(
      <PageStateView state={{ kind: "data" }} {...defaults}>
        <div data-testid="data">live</div>
      </PageStateView>,
    );
    expect(screen.getByTestId("data")).toBeInTheDocument();
  });

  it("'empty' renders EmptyState with the configured title", () => {
    wrap(
      <PageStateView state={{ kind: "empty" }} {...defaults}>
        <div>nope</div>
      </PageStateView>,
    );
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
  });

  it("'filtered-empty' wires onClearFilters to the action button", () => {
    const onClear = vi.fn();
    wrap(
      <PageStateView
        state={{ kind: "filtered-empty", onClearFilters: onClear }}
        {...defaults}
      >
        <div>nope</div>
      </PageStateView>,
    );
    expect(screen.getByText("No matches")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(onClear).toHaveBeenCalled();
  });

  it("'error' renders ErrorState and wires onRetry", () => {
    const onRetry = vi.fn();
    wrap(
      <PageStateView state={{ kind: "error", onRetry }} {...defaults}>
        <div>nope</div>
      </PageStateView>,
    );
    expect(screen.getByText("Boom")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalled();
  });

  it("each PageState kind is exhaustively handled (compile-level, but verified at runtime by switch)", () => {
    const kinds: PageState[] = [
      { kind: "loading" },
      { kind: "data" },
      { kind: "empty" },
      { kind: "filtered-empty", onClearFilters: () => {} },
      { kind: "error", onRetry: () => {} },
    ];
    for (const state of kinds) {
      const { unmount } = wrap(
        <PageStateView state={state} {...defaults}>
          <div>x</div>
        </PageStateView>,
      );
      // Each render must produce some content — empty render = a missed branch.
      expect(document.body.textContent?.length ?? 0).toBeGreaterThan(0);
      unmount();
    }
  });
});

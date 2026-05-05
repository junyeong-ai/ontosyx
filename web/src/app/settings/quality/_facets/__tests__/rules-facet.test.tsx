import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

// Mock `request` before import so the `load()` effect reads the
// spy — the page calls `/quality/dashboard` on mount.
vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
}));

// `useConfirm` returns a function — stub to a vi.fn() that can be
// toggled per-test. Stored as a module-local so tests reach into
// it directly.
const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { RulesFacet } from "@/app/settings/quality/_facets/rules-facet";
import { request } from "@/lib/api/client";
import { toast } from "@/components/ui/toast";

const SAMPLE_RULE = {
  rule_id: "r-1",
  name: "Customer email completeness",
  rule_type: "completeness",
  target_label: "Customer",
  target_property: "email",
  severity: "warning",
  threshold: 95,
  cypher_check: null,
  latest_passed: true,
  latest_value: 97.2,
  latest_evaluated_at: "2026-04-23T00:00:00Z",
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <RulesFacet />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("RulesFacet", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (request as ReturnType<typeof vi.fn>).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("renders summary counts from the dashboard", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      SAMPLE_RULE,
      {
        ...SAMPLE_RULE,
        rule_id: "r-2",
        name: "Order freshness",
        latest_passed: false,
      },
      {
        ...SAMPLE_RULE,
        rule_id: "r-3",
        name: "Unevaluated",
        latest_passed: null,
      },
    ]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Customer email completeness")).toBeInTheDocument(),
    );
    // The three summary cards count pass/fail/pending rules.
    // Use text() search on the three unique labels so we don't rely
    // on order of nodes in the DOM.
    expect(screen.getByText("Passing")).toBeInTheDocument();
    expect(screen.getByText("Failing")).toBeInTheDocument();
    expect(screen.getByText("Not Yet Evaluated")).toBeInTheDocument();
  });

  it("reveals the cypher-check textarea only when rule type is custom", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    renderPage();
    // Open the create form — the "Create Rule" button renders twice
    // (header CTA + empty-state CTA), both are equivalent.
    await waitFor(() =>
      expect(
        screen.getAllByRole("button", { name: /Create Rule/ })[0],
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getAllByRole("button", { name: /Create Rule/ })[0]);
    // Initial rule_type = "completeness" → no cypher textarea.
    expect(
      screen.queryByPlaceholderText(/MATCH \(n:Label\)/),
    ).not.toBeInTheDocument();
    // Switch to custom.
    const typeSelect = screen.getAllByRole("combobox")[0];
    fireEvent.change(typeSelect, { target: { value: "custom" } });
    expect(
      screen.getByPlaceholderText(/MATCH \(n:Label\)/),
    ).toBeInTheDocument();
    // Switch back — textarea hides again.
    fireEvent.change(typeSelect, { target: { value: "completeness" } });
    expect(
      screen.queryByPlaceholderText(/MATCH \(n:Label\)/),
    ).not.toBeInTheDocument();
  });

  it("blocks submission until name + target_label are non-empty", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getAllByRole("button", { name: /Create Rule/ })[0],
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getAllByRole("button", { name: /Create Rule/ })[0]);
    // Form's submit button is "Create Rule" too — pick the one
    // inside the form scope by finding the second occurrence.
    const submitButton = screen
      .getAllByRole("button", { name: /^Create Rule$/ })
      .pop() as HTMLButtonElement;
    expect(submitButton.disabled).toBe(true);

    // Fill name only — still disabled (target_label is empty).
    fireEvent.change(screen.getByPlaceholderText(/Brand completeness/), {
      target: { value: "My rule" },
    });
    expect(submitButton.disabled).toBe(true);

    // Fill target_label — now enabled.
    fireEvent.change(screen.getByPlaceholderText(/Brand, Product/), {
      target: { value: "Customer" },
    });
    expect(submitButton.disabled).toBe(false);
  });

  it("execute single rule POSTs and fires a pass/fail toast with the value", async () => {
    (request as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([SAMPLE_RULE]) // /quality/dashboard
      .mockResolvedValueOnce({
        id: "result-1",
        workspace_id: "ws-1",
        rule_id: "r-1",
        passed: true,
        actual_value: 96.7,
        details: {},
        evaluated_at: "2026-04-23T00:00:00Z",
      })
      .mockResolvedValueOnce([SAMPLE_RULE]); // reload after execute
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(SAMPLE_RULE.name)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Run$/ }));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        expect.stringContaining("96.7"),
      ),
    );
    expect(request).toHaveBeenCalledWith(
      `/quality/rules/${SAMPLE_RULE.rule_id}/execute`,
      expect.objectContaining({ method: "POST" }),
    );
  });
});

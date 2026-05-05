import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { ScopeBadge } from "@/components/workbench/design/scope-badge";
import * as projectsApi from "@/lib/api/ontology-drafts";
import { useAppStore } from "@/lib/store";
import type { OntologyDraft } from "@/types/api";

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function fixtureProject(): OntologyDraft {
  return {
    id: "proj-1",
    revision: 7,
    analysis_scope: {
      included: ["customers", "orders"],
      deferred: [
        {
          table: "audit_log",
          reason: "deferred at bootstrap",
          deferred_at: "2026-05-01T00:00:00Z",
        },
      ],
      excluded_by_policy: [],
      fingerprints: {},
      last_introspected_at: "2026-05-01T00:00:00Z",
    },
  } as unknown as OntologyDraft;
}

function renderBadge() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <ScopeBadge />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  return render(ui);
}

describe("ScopeBadge", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAppStore.getState().applyProjectSnapshot(fixtureProject());
  });

  it("renders summary with included + deferred counts", () => {
    renderBadge();
    expect(screen.getByText(/Modeled 2/)).toBeDefined();
    expect(screen.getByText(/Deferred 1/)).toBeDefined();
  });

  it("opens a popover that lists modeled and deferred tables", () => {
    renderBadge();
    // Click the badge button to open the popover.
    fireEvent.click(screen.getByText(/Modeled 2/));

    expect(screen.getByText("customers")).toBeDefined();
    expect(screen.getByText("orders")).toBeDefined();
    expect(screen.getByText("audit_log")).toBeDefined();
    expect(screen.getByText("deferred at bootstrap")).toBeDefined();
  });

  it("Promote on a deferred row fires includeScopeTables with project revision", async () => {
    const includeSpy = vi
      .spyOn(projectsApi, "includeScopeTables")
      .mockResolvedValue({
        project: fixtureProject(),
      });

    renderBadge();
    fireEvent.click(screen.getByText(/Modeled 2/));
    // The deferred row's button is "Promote".
    fireEvent.click(screen.getByRole("button", { name: /^Promote$/ }));

    await waitFor(() => expect(includeSpy).toHaveBeenCalled());
    expect(includeSpy).toHaveBeenCalledWith("proj-1", {
      tables: ["audit_log"],
      expected_revision: 7,
    });
  });

  it("Defer + reason input fires deferScopeTables with the typed reason", async () => {
    const deferSpy = vi
      .spyOn(projectsApi, "deferScopeTables")
      .mockResolvedValue({
        project: fixtureProject(),
      });

    renderBadge();
    fireEvent.click(screen.getByText(/Modeled 2/));

    // Click "Defer" on the first modeled row (customers).
    const deferButtons = screen.getAllByRole("button", { name: /^Defer$/ });
    expect(deferButtons.length).toBeGreaterThan(0);
    fireEvent.click(deferButtons[0]);

    // The row's action button area is replaced with a reason input
    // + Save / cancel. Type a reason and click Save.
    const reasonInput = screen.getByPlaceholderText(/Reason/i);
    fireEvent.change(reasonInput, { target: { value: "out of scope" } });
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));

    await waitFor(() => expect(deferSpy).toHaveBeenCalled());
    const [projectId, payload] = deferSpy.mock.calls[0] ?? [];
    expect(projectId).toBe("proj-1");
    expect(payload).toEqual({
      tables: ["customers"],
      reason: "out of scope",
      expected_revision: 7,
    });
  });

  it("returns null when there is no active project", () => {
    useAppStore.getState().applyProjectSnapshot(null);
    const { container } = renderBadge();
    expect(container.textContent).toBe("");
  });
});

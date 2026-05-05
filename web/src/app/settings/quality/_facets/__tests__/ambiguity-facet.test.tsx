import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

vi.mock("@/lib/api/ambiguity", () => ({
  listAmbiguities: vi.fn(),
  resolveAmbiguity: vi.fn(),
  revokeAmbiguity: vi.fn(),
  bulkRevokeAmbiguities: vi.fn(),
  getAmbiguity: vi.fn(),
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { AmbiguityFacet } from "@/app/settings/quality/_facets/ambiguity-facet";
import {
  bulkRevokeAmbiguities,
  listAmbiguities,
  resolveAmbiguity,
  revokeAmbiguity,
} from "@/lib/api/ambiguity";
import { toast } from "@/components/ui/toast";

const PENDING = {
  context: {
    id: "ctx-pending",
    source_id: "src-postgres",
    column: { relation: "orders", column: "status" },
    kind: { kind: "numeric_code" as const },
    sample_values: ["1"],
    clarification_prompt: "What do these codes mean?",
    detection_source_hash: "sha256:abc",
    detected_at: "2026-04-22T00:00:00Z",
  },
  active_resolution: null,
};

const RESOLVED = {
  context: {
    id: "ctx-resolved",
    source_id: "src-postgres",
    column: { relation: "users", column: "tier" },
    kind: { kind: "overloaded_name" as const },
    sample_values: ["gold"],
    clarification_prompt: "Loyalty tier?",
    detection_source_hash: "sha256:fixed",
    detected_at: "2026-04-22T00:00:00Z",
  },
  active_resolution: {
    id: "r-1",
    context_id: "ctx-resolved",
    context_source_hash: "sha256:fixed",
    mapping: {
      kind: "code_system_ref" as const,
      code_system_id: "cs-tier",
    },
    resolved_at: "2026-04-22T00:00:00Z",
  },
};

const STALE = {
  context: {
    id: "ctx-stale",
    source_id: "src-mysql",
    column: { relation: "items", column: "category" },
    kind: { kind: "opaque_short_code" as const },
    sample_values: ["a"],
    clarification_prompt: "Category code?",
    detection_source_hash: "sha256:new",
    detected_at: "2026-04-22T00:00:00Z",
  },
  active_resolution: {
    id: "r-2",
    context_id: "ctx-stale",
    context_source_hash: "sha256:old",
    mapping: { kind: "glossary_ref" as const, term_id: "g-cat" },
    resolved_at: "2026-04-22T00:00:00Z",
  },
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <AmbiguityFacet />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("AmbiguityFacet", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (listAmbiguities as ReturnType<typeof vi.fn>).mockReset();
    (resolveAmbiguity as ReturnType<typeof vi.fn>).mockReset();
    (revokeAmbiguity as ReturnType<typeof vi.fn>).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
  });

  it("classifies summaries into pending / stale / resolved tab counts", async () => {
    (listAmbiguities as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [PENDING, RESOLVED, STALE],
    });
    renderPage();

    // Default tab is "pending" — its row renders.
    await waitFor(() => expect(screen.getByText("orders.")).toBeInTheDocument());

    // Each tab button shows its bucket count badge.
    const tabs = screen.getAllByRole("button", { pressed: false });
    // Counts: pending 1, stale 1, resolved 1 — find each tab via its label.
    const pendingTab = screen.getByRole("button", { name: /Needs attention/ });
    const staleTab = screen.getByRole("button", { name: /Stale/ });
    const resolvedTab = screen.getByRole("button", { name: /Resolved/ });
    expect(pendingTab.textContent).toContain("1");
    expect(staleTab.textContent).toContain("1");
    expect(resolvedTab.textContent).toContain("1");
    expect(tabs.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the empty-state copy when the bucket has no rows", async () => {
    (listAmbiguities as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [RESOLVED],
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/Nothing to resolve/),
      ).toBeInTheDocument(),
    );
  });

  it("Revoke button on a resolved row calls the API", async () => {
    (listAmbiguities as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [RESOLVED],
    });
    (revokeAmbiguity as ReturnType<typeof vi.fn>).mockResolvedValue({
      revoked: true,
    });
    renderPage();
    // Switch to resolved tab.
    fireEvent.click(screen.getByRole("button", { name: /Resolved/ }));
    await waitFor(() => expect(screen.getByText("users.")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /Revoke/ }));
    await waitFor(() =>
      expect(revokeAmbiguity).toHaveBeenCalledWith("ctx-resolved"),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Resolution revoked"),
    );
  });

  it("bulk-revokes selected resolved ambiguities", async () => {
    const RESOLVED_2 = {
      ...RESOLVED,
      context: { ...RESOLVED.context, id: "ctx-resolved-2" },
      active_resolution: {
        ...RESOLVED.active_resolution,
        id: "r-3",
        context_id: "ctx-resolved-2",
      },
    };
    (listAmbiguities as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [RESOLVED, RESOLVED_2],
    });
    (bulkRevokeAmbiguities as ReturnType<typeof vi.fn>).mockResolvedValue({
      revoked: 2,
    });
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: /Resolved/ }));
    // Wait for the resolved tab to render rows + bulk header checkbox.
    await waitFor(() =>
      expect(
        screen.getByLabelText("Select all resolved ambiguities"),
      ).toBeInTheDocument(),
    );
    // Tick the select-all header checkbox.
    fireEvent.click(
      screen.getByLabelText("Select all resolved ambiguities"),
    );
    // BulkActionBar slides in — its Revoke button (region role).
    const bar = await screen.findByRole("region", { name: /Bulk actions/ });
    fireEvent.click(
      Array.from(bar.querySelectorAll("button")).find(
        (b) => b.textContent === "Revoke",
      ) as HTMLButtonElement,
    );
    await waitFor(() =>
      expect(bulkRevokeAmbiguities).toHaveBeenCalledWith([
        "ctx-resolved",
        "ctx-resolved-2",
      ]),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("2 resolutions revoked"),
    );
  });

  it("clicking Resolve on a pending row opens the resolution modal", async () => {
    (listAmbiguities as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [PENDING],
    });
    renderPage();
    await waitFor(() => expect(screen.getByText("orders.")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /^Resolve$/ }));
    // Modal heading interpolates relation + column.
    await waitFor(() =>
      expect(
        screen.getByText(/Resolve orders\.status/),
      ).toBeInTheDocument(),
    );
  });
});

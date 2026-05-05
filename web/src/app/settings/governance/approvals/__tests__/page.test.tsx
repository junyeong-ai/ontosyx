import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

// Mock `request` before importing the page — the page reaches into
// `@/lib/api/client` at module load via `useCallback(load)`, so the
// mock has to be in place before the dynamic import resolves.
vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
}));

// Sonner toast — asserted on for success/failure paths.
vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import ApprovalsSettingsPage from "@/app/settings/governance/approvals/page";
import { request } from "@/lib/api/client";
import { toast } from "@/components/ui/toast";

const MOCK_PENDING = {
  id: "p-1",
  requester_id: "u-1",
  requester_name: "Bob",
  action_type: "schema_deploy",
  resource_type: "ontology",
  resource_id: "aaaaaaaa-1111-2222-3333-444444444444",
  status: "pending",
  reviewer_id: null,
  reviewer_name: null,
  reviewed_at: null,
  expires_at: "2026-05-01T00:00:00Z",
  created_at: "2026-04-23T00:00:00Z",
};

const MOCK_RESOLVED = {
  id: "r-1",
  requester_id: "u-2",
  requester_name: "Carol",
  action_type: "tool_run",
  resource_type: "project",
  resource_id: "bbbbbbbb-1111-2222-3333-444444444444",
  status: "approved",
  reviewer_id: "u-admin",
  reviewer_name: "Admin",
  reviewed_at: "2026-04-22T01:00:00Z",
  expires_at: "2026-05-01T00:00:00Z",
  created_at: "2026-04-22T00:00:00Z",
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <ApprovalsSettingsPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("ApprovalsSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (request as ReturnType<typeof vi.fn>).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
  });

  it("splits pending + resolved entries into separate sections", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      MOCK_PENDING,
      MOCK_RESOLVED,
    ]);
    renderPage();
    // Pending heading uses `{count}` — assert the "(1)" from the
    // one pending row flows through the i18n substitution.
    await waitFor(() =>
      expect(screen.getByText(/Pending \(1\)/)).toBeInTheDocument(),
    );
    expect(screen.getByText("History")).toBeInTheDocument();
    // Resolved badge label is rendered as-is for known statuses.
    expect(screen.getByText("Approved")).toBeInTheDocument();
  });

  it("Approve button POSTs { approved: true } and fires success toast", async () => {
    (request as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([MOCK_PENDING])
      .mockResolvedValueOnce(undefined) // review POST
      .mockResolvedValueOnce([MOCK_PENDING]); // reload
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(/Pending \(1\)/)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /Approve/ }));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Approved"),
    );
    expect(request).toHaveBeenCalledWith(
      `/approvals/${MOCK_PENDING.id}/review`,
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ approved: true }),
      }),
    );
  });

  it("Reject button POSTs { approved: false } and fires rejection toast", async () => {
    (request as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([MOCK_PENDING])
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Reject/ })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /Reject/ }));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Rejected"),
    );
    const body = JSON.parse(
      (request as ReturnType<typeof vi.fn>).mock.calls[1][1].body as string,
    );
    expect(body).toEqual({ approved: false });
  });

  it("swallows malformed list response into an empty page", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      "not-an-array" as unknown as unknown[],
    );
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Approval Queue")).toBeInTheDocument(),
    );
    // No pending heading — the filter on a zero-length array
    // should hide the `{count > 0}` section entirely.
    expect(screen.queryByText(/Pending \(/)).not.toBeInTheDocument();
  });
});

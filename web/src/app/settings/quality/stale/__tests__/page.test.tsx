import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

vi.mock("@/lib/api/quality", async () => {
  const actual = await vi.importActual<Record<string, unknown>>(
    "@/lib/api/quality",
  );
  return {
    ...actual,
    listStaleProposals: vi.fn(),
    decideStaleProposal: vi.fn(),
    listTypeCandidates: vi.fn(),
  };
});

vi.mock("@/lib/api/binding-suggestions", async () => {
  const actual = await vi.importActual<Record<string, unknown>>(
    "@/lib/api/binding-suggestions",
  );
  return {
    ...actual,
    applyOntologyEdits: vi.fn(),
  };
});

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

import StaleConceptsPage from "@/app/settings/quality/stale/page";
import {
  listStaleProposals,
  decideStaleProposal,
  listTypeCandidates,
} from "@/lib/api/quality";
import { toast } from "sonner";
import type { StaleConceptProposal } from "@/types/api";

const PENDING: StaleConceptProposal = {
  id: "p-1",
  workspace_id: "ws-1",
  type_id: "Customer",
  type_kind: "node",
  last_used_at: null,
  days_since_last_use: 320,
  proposed_at: "2026-04-22T00:00:00Z",
  decision: "pending",
  decided_at: null,
  decided_by_user_id: null,
  reason: null,
};

const APPROVED: StaleConceptProposal = {
  ...PENDING,
  id: "p-2",
  type_id: "OldEdge",
  decision: "approved",
  decided_at: "2026-04-22T01:00:00Z",
  reason: "replaced",
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <StaleConceptsPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("StaleConceptsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(listStaleProposals).mockReset();
    vi.mocked(decideStaleProposal).mockReset();
    vi.mocked(listTypeCandidates).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    (toast.info as ReturnType<typeof vi.fn>).mockReset();
  });

  it("renders pending rows by default + history tab is reachable", async () => {
    // pending=false call returns only pending, includeDecided=true returns both.
    vi.mocked(listStaleProposals).mockImplementation((includeDecided) =>
      Promise.resolve(includeDecided ? [PENDING, APPROVED] : [PENDING]),
    );
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("Customer")).toBeInTheDocument(),
    );
    // History tab carries "(1)"-like badge from the decided derived list.
    fireEvent.click(screen.getByRole("button", { name: /History/ }));
    await waitFor(() => expect(screen.getByText("OldEdge")).toBeInTheDocument());
    expect(screen.getByText("Approved")).toBeInTheDocument();
  });

  it("clicking Review opens the decision modal with the proposal id", async () => {
    vi.mocked(listStaleProposals).mockResolvedValue([PENDING]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Customer")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /Review/ }));
    await waitFor(() =>
      expect(
        screen.getByText(/Review stale-concept proposal/),
      ).toBeInTheDocument(),
    );
    // modal subtitle interpolates idle days.
    expect(screen.getByText(/Idle for 320 days/)).toBeInTheDocument();
  });

  it("Dismiss path posts decision=dismissed without invoking type lookup", async () => {
    vi.mocked(listStaleProposals).mockResolvedValue([PENDING]);
    vi.mocked(decideStaleProposal).mockResolvedValue({
      ...PENDING,
      decision: "dismissed",
      decided_at: "2026-04-22T01:00:00Z",
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Customer")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /Review/ }));
    await waitFor(() =>
      expect(
        screen.getByText(/Review stale-concept proposal/),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Dismiss$/ }));

    await waitFor(() =>
      expect(decideStaleProposal).toHaveBeenCalledWith(
        "p-1",
        "dismissed",
        undefined,
      ),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Proposal dismissed"),
    );
    // No follow-up lookup on dismissal.
    expect(listTypeCandidates).not.toHaveBeenCalled();
  });

  it("Approve path triggers a candidate lookup and surfaces noCandidate toast on empty result", async () => {
    vi.mocked(listStaleProposals).mockResolvedValue([PENDING]);
    vi.mocked(decideStaleProposal).mockResolvedValue({
      ...PENDING,
      decision: "approved",
      decided_at: "2026-04-22T01:00:00Z",
    });
    vi.mocked(listTypeCandidates).mockResolvedValue([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Customer")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /Review/ }));
    await waitFor(() =>
      expect(
        screen.getByText(/Review stale-concept proposal/),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Approve$/ }));

    await waitFor(() =>
      expect(decideStaleProposal).toHaveBeenCalledWith(
        "p-1",
        "approved",
        undefined,
      ),
    );
    await waitFor(() =>
      expect(listTypeCandidates).toHaveBeenCalledWith("Customer", "node"),
    );
    await waitFor(() =>
      expect(toast.info).toHaveBeenCalledWith(
        "Approved — no ontology in this workspace carries this type",
      ),
    );
  });
});

import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("@/lib/api/knowledge", () => ({
  listKnowledge: vi.fn(),
  deleteKnowledge: vi.fn(),
  updateKnowledgeStatus: vi.fn(),
  knowledgeStats: vi.fn(),
  bulkReviewKnowledge: vi.fn(),
}));

const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
  usePathname: () => "/knowledge",
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import KnowledgePage from "@/app/(workbench)/knowledge/page";
import {
  listKnowledge,
  deleteKnowledge,
  updateKnowledgeStatus,
  bulkReviewKnowledge,
} from "@/lib/api/knowledge";
import type { KnowledgeEntry } from "@/types/api";
import { useAuth } from "@/hooks/use-auth";
import { mockAuth } from "@/test-utils/auth";
import { toast } from "@/components/ui/toast";

function sampleEntry(overrides: Partial<KnowledgeEntry> = {}): KnowledgeEntry {
  return {
    id: "kb-1",
    workspace_id: "ws-1",
    ontology_name: "orders",
    ontology_version_min: 1,
    ontology_version_max: null,
    kind: "correction",
    status: "draft",
    confidence: 0.9,
    title: "Prefer customer_id over user_id when joining orders",
    content: "Use customer_id — user_id is a legacy alias.",
    structured_data: {},
    version_checked: 1,
    content_hash: "sha256:abc",
    source_execution_ids: [],
    source_session_id: null,
    affected_labels: ["Order", "Customer"],
    affected_properties: [],
    use_count: 0,
    last_used_at: null,
    created_by: "user-a",
    reviewed_by: null,
    reviewed_at: null,
    review_notes: null,
    created_at: "2026-04-22T00:00:00Z",
    ...overrides,
  } as KnowledgeEntry;
}

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <KnowledgePage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

function asAdmin(): void {
  vi.mocked(useAuth).mockReturnValue(
    mockAuth(
      { kind: "authenticated", role: "admin" },
      { sub: "u1", email: "a@b.c", name: "Admin" },
    ),
  );
}

describe("KnowledgePage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (listKnowledge as ReturnType<typeof vi.fn>).mockReset();
    (deleteKnowledge as ReturnType<typeof vi.fn>).mockReset();
    (updateKnowledgeStatus as ReturnType<typeof vi.fn>).mockReset();
    (bulkReviewKnowledge as ReturnType<typeof vi.fn>).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("shows the admin-only placeholder when the viewer lacks admin rights", async () => {
    vi.mocked(useAuth).mockReturnValue(
      mockAuth({ kind: "authenticated", role: "viewer" }),
    );
    // The hook sits above the gate, so listKnowledge fires — the gate
    // only controls what's rendered, not whether the query runs.
    (listKnowledge as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(/Admin access required/)).toBeInTheDocument(),
    );
    // Row content never appears because the gate short-circuits the render.
    expect(
      screen.queryByText(/Prefer customer_id/),
    ).not.toBeInTheDocument();
  });

  it("renders the entry list with ontology title when the API returns rows", async () => {
    asAdmin();
    (listKnowledge as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [sampleEntry()],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(
          /Prefer customer_id over user_id when joining orders/,
        ),
      ).toBeInTheDocument(),
    );
  });

  it("renders the empty-state card when the API returns zero rows", async () => {
    asAdmin();
    (listKnowledge as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [],
      next_cursor: undefined,
    });
    renderPage();
    // Knowledge has its own empty hint — match any settings.knowledge.empty*
    // copy that includes the word "entries" or "No".
    await waitFor(() => {
      const empty = screen.getByText(/No knowledge entries/i);
      expect(empty).toBeInTheDocument();
    });
  });

  it("confirm=true on delete fires deleteKnowledge with the entry id", async () => {
    asAdmin();
    (listKnowledge as ReturnType<typeof vi.fn>).mockResolvedValue({
      items: [sampleEntry()],
      next_cursor: undefined,
    });
    (deleteKnowledge as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    confirmMock.mockResolvedValueOnce(true);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(
          /Prefer customer_id over user_id when joining orders/,
        ),
      ).toBeInTheDocument(),
    );
    // Expand the card to surface the action buttons.
    fireEvent.click(
      screen.getByText(
        /Prefer customer_id over user_id when joining orders/,
      ),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /^Delete$/ }),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() =>
      expect(deleteKnowledge).toHaveBeenCalledWith("kb-1"),
    );
  });
});

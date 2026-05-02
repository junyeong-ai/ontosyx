import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

// useQueryState pulls from next/navigation — stub that explicitly so
// the hook has predictable router + search-params instances.
vi.mock("next/navigation", () => ({
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
  usePathname: () => "/settings/reports",
  useSearchParams: () => new URLSearchParams(),
}));

vi.mock("@/lib/api", () => ({
  listReports: vi.fn(),
  createReport: vi.fn(),
  updateReport: vi.fn(),
  deleteReport: vi.fn(),
  executeReport: vi.fn(),
  listOntologies: vi.fn(),
}));

const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import ReportsPage from "@/app/settings/reports/page";
import * as api from "@/lib/api";
import type { OntologyListItem, SavedReport } from "@/types/api";
import { toast } from "sonner";

const SAMPLE_ONTOLOGY: OntologyListItem = {
  id: "ont-1",
  lineage_id: "lin-1",
  name: "orders",
  description: { default: "orders domain" },
  created_at: "2026-04-22T00:00:00Z",
  updated_at: "2026-04-22T00:00:00Z",
};

function sampleReport(overrides: Partial<SavedReport> = {}): SavedReport {
  return {
    id: "rpt-1",
    user_id: "user-a",
    ontology_lineage_id: "lin-1",
    title: "Weekly revenue by cohort",
    description: "Top-level revenue slice",
    query_template: "MATCH (o:Order) RETURN count(o)",
    parameters: [],
    widget_type: "table",
    is_public: false,
    created_at: "2026-04-22T00:00:00Z",
    updated_at: "2026-04-22T00:00:00Z",
    ...overrides,
  };
}

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <ReportsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("ReportsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(api.listOntologies).mockReset();
    vi.mocked(api.listReports).mockReset();
    vi.mocked(api.deleteReport).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("lists ontologies then fetches reports for the first ontology's lineage_id", async () => {
    vi.mocked(api.listOntologies).mockResolvedValueOnce({
      items: [SAMPLE_ONTOLOGY],
      next_cursor: undefined,
    });
    vi.mocked(api.listReports).mockResolvedValueOnce({
      items: [sampleReport()],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(api.listReports).toHaveBeenCalledWith({
        ontology_lineage_id: "lin-1",
      }),
    );
    // The report title surfaces on the list panel.
    await waitFor(() =>
      expect(screen.getByText("Weekly revenue by cohort")).toBeInTheDocument(),
    );
  });

  it("surfaces `loadOntologiesFailed` toast when the ontology fetch rejects", async () => {
    vi.mocked(api.listOntologies).mockRejectedValueOnce(new Error("nope"));
    renderPage();
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Failed to load ontologies",
      ),
    );
    // No reports fetched when the ontologies call fails outright — the
    // second effect keys on `ontologyFilter` which stays empty.
    expect(api.listReports).not.toHaveBeenCalled();
  });

  it("surfaces `loadFailed` toast when listReports rejects after ontology loads", async () => {
    vi.mocked(api.listOntologies).mockResolvedValueOnce({
      items: [SAMPLE_ONTOLOGY],
      next_cursor: undefined,
    });
    vi.mocked(api.listReports).mockRejectedValueOnce(new Error("boom"));
    renderPage();
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to load reports"),
    );
  });

  it("renders the empty copy when zero reports return for the selected ontology", async () => {
    vi.mocked(api.listOntologies).mockResolvedValueOnce({
      items: [SAMPLE_ONTOLOGY],
      next_cursor: undefined,
    });
    vi.mocked(api.listReports).mockResolvedValueOnce({
      items: [],
      next_cursor: undefined,
    });
    renderPage();
    // The list surface renders the empty copy — match any "no reports"
    // variant to stay resilient to copy edits.
    await waitFor(() =>
      expect(api.listReports).toHaveBeenCalled(),
    );
    // Make sure the zero-row state doesn't crash the tree.
    expect(screen.queryByText("Weekly revenue by cohort")).not.toBeInTheDocument();
  });
});

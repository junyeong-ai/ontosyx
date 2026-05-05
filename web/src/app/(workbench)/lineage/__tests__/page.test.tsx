import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

// Lineage gates on the workspace's canonical ontology — without one
// the page renders the empty-state instead of the data view. Stub the
// hook to a populated canonical so the data-path tests run as before.
vi.mock("@/hooks/api/use-workspace-ontology", () => ({
  useWorkspaceOntology: () => ({
    data: {
      id: "ont-1",
      lineage_id: "lin-1",
      name: "test",
      description: { default: "" },
      current_version: {
        version_id: "v1",
        version: "1",
        committed_by: "u",
        commit_message: "init",
        created_at: "2026-04-22T00:00:00Z",
      },
    },
    isLoading: false,
    isError: false,
    refetch: vi.fn(),
  }),
}));

import LineageSettingsPage from "@/app/(workbench)/lineage/page";
import { request } from "@/lib/api/client";
import { toast } from "@/components/ui/toast";

const SUMMARY_ROW = {
  graph_label: "Order",
  graph_element_type: "node",
  source_count: 1,
  total_records: 1500,
  last_loaded_at: "2026-04-22T09:00:00Z",
};

const ENTRY = {
  id: "lin-1",
  graph_label: "Order",
  graph_element_type: "node",
  source_type: "postgres",
  source_name: "warehouse",
  source_table: "orders",
  source_columns: ["id", "customer_id"],
  property_mappings: [
    {
      label: "Order",
      element_type: "node",
      mappings: [
        {
          source_column: "id",
          graph_property: "order_id",
          transform: null,
          mapping_kind: "match",
        },
        {
          source_column: "customer_id",
          graph_property: "customerId",
          transform: null,
          mapping_kind: "set",
        },
      ],
    },
  ],
  record_count: 1500,
  started_at: "2026-04-22T09:00:00Z",
  completed_at: "2026-04-22T09:05:00Z",
  status: "completed",
  error_message: null,
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <LineageSettingsPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("LineageSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (request as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
  });

  it("renders summary tiles and the history row with formatted counts", async () => {
    // Summary call first, then one per unique label.
    (request as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([SUMMARY_ROW])
      .mockResolvedValueOnce([ENTRY]);
    renderPage();
    // `Order` appears in multiple places (mapping card + history row).
    await waitFor(() =>
      expect(screen.getAllByText("Order").length).toBeGreaterThan(0),
    );
    // 1500 → "1.5천" — Intl compact-notation against the default
    // ko-first locale chain. en chain would render "1.5K"; the ko unit
    // is the correct localised abbreviation for the workspace.
    expect(screen.getAllByText("1.5천").length).toBeGreaterThan(0);
    // Both request calls fired — summary + per-label.
    expect(request).toHaveBeenCalledWith("/lineage");
    expect(request).toHaveBeenCalledWith("/lineage/label/Order");
  });

  it("renders the empty history row when zero entries return", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      // `noHistory` copy surfaces only when entries[] is empty.
      expect(
        screen.getByText(/No load history available/i),
      ).toBeInTheDocument(),
    );
    // With zero summary rows, only the summary fetch runs — the
    // per-label loop short-circuits.
    expect(request).toHaveBeenCalledTimes(1);
  });

  it("renders the aggregated mapping card grouping source_table → graph_label", async () => {
    (request as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([SUMMARY_ROW])
      .mockResolvedValueOnce([ENTRY]);
    renderPage();
    // `orders` appears twice (mapping card summary + history table cell).
    await waitFor(() =>
      expect(screen.getAllByText("orders").length).toBeGreaterThan(0),
    );
    // `columnMappings` h2 renders because aggregateMappings returns a group.
    expect(screen.getByText("Column Mappings")).toBeInTheDocument();
  });

  it("shows the loadFailed toast when the summary call rejects", async () => {
    (request as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error("boom"),
    );
    renderPage();
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Failed to load lineage data",
      ),
    );
  });
});

import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";
import { ConfirmProvider } from "@/components/ui/confirm-dialog";

import messages from "../../../../../../messages/en.json";

vi.mock("@/lib/api/quality", async () => {
  const actual = await vi.importActual<Record<string, unknown>>(
    "@/lib/api/quality",
  );
  return {
    ...actual,
    getQualityMetrics: vi.fn(),
    listShaclFailures: vi.fn(),
    listStaleTypes: vi.fn(),
  };
});

import QualitySignalsPage from "@/app/settings/quality/signals/page";
import {
  getQualityMetrics,
  listShaclFailures,
  listStaleTypes,
} from "@/lib/api/quality";

function makeMetric(
  value: number,
  trend = 0,
  band: [number, number] = [value - 0.05, value + 0.05],
) {
  return {
    value,
    trend_delta: trend,
    lower_bound_95: band[0],
    upper_bound_95: band[1],
  };
}

const FULL_REPORT = {
  anchor_match_rate: makeMetric(0.62, 0.012),
  glossary_hit_rate: makeMetric(0.71, -0.008),
  clarification_success_rate: makeMetric(0.55, 0.002),
  query_reproducibility: makeMetric(0.84, 0.0),
  shacl_pass_rate: makeMetric(0.93, 0.005),
  stale_concept_ratio: makeMetric(0.18, -0.02),
  sample_size: 412,
  window: "last7d" as const,
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        {/* ConfirmProvider is required because the stale table now
            calls `useConfirm()` for the deprecate-proposal flow
            (Φ5 #4). The test never opens the confirm dialog;
            mounting the provider lets the hook resolve without
            throwing. */}
        <ConfirmProvider>
          <QualitySignalsPage />
        </ConfirmProvider>
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("QualitySignalsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(getQualityMetrics).mockReset();
    vi.mocked(listShaclFailures).mockReset();
    vi.mocked(listStaleTypes).mockReset();
  });

  it("renders the six metric tiles with their percent value", async () => {
    vi.mocked(getQualityMetrics).mockResolvedValue(FULL_REPORT);
    vi.mocked(listShaclFailures).mockResolvedValue([]);
    vi.mocked(listStaleTypes).mockResolvedValue([]);
    renderPage();

    // Six tile labels.
    await waitFor(() =>
      expect(screen.getByText("Anchor match rate")).toBeInTheDocument(),
    );
    expect(screen.getByText("Glossary hit rate")).toBeInTheDocument();
    expect(screen.getByText("Clarification success rate")).toBeInTheDocument();
    expect(screen.getByText("Query reproducibility")).toBeInTheDocument();
    expect(screen.getByText("SHACL pass rate")).toBeInTheDocument();
    expect(screen.getByText("Stale concept ratio")).toBeInTheDocument();

    // 0.62 → "62.0%" rendered into the anchor tile.
    expect(screen.getByText("62.0%")).toBeInTheDocument();
    // sample_size 412 surfaces.
    expect(screen.getByText(/n = 412/)).toBeInTheDocument();
  });

  it("changing the time-window selector triggers a re-fetch with the new window", async () => {
    vi.mocked(getQualityMetrics).mockResolvedValue(FULL_REPORT);
    vi.mocked(listShaclFailures).mockResolvedValue([]);
    vi.mocked(listStaleTypes).mockResolvedValue([]);
    renderPage();

    await waitFor(() =>
      expect(getQualityMetrics).toHaveBeenCalledWith("7d"),
    );
    // Time-window <select>.
    fireEvent.change(screen.getByDisplayValue("Last 7 days"), {
      target: { value: "30d" },
    });
    await waitFor(() =>
      expect(getQualityMetrics).toHaveBeenCalledWith("30d"),
    );
    expect(listShaclFailures).toHaveBeenCalledWith("30d");
  });

  it("renders SHACL failure bars when the failure list is non-empty", async () => {
    vi.mocked(getQualityMetrics).mockResolvedValue(FULL_REPORT);
    vi.mocked(listShaclFailures).mockResolvedValue([
      { kind: "cardinality_violation", count: 12 },
      { kind: "measure_group_by", count: 4 },
    ]);
    vi.mocked(listStaleTypes).mockResolvedValue([]);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("Cardinality violation")).toBeInTheDocument(),
    );
    expect(screen.getByText("Measure in GROUP BY")).toBeInTheDocument();
    // count + percent formatting: "12 · 75.0%".
    expect(screen.getByText(/12 · 75\.0%/)).toBeInTheDocument();
  });

  it("renders the stale-types table when entries are returned", async () => {
    vi.mocked(getQualityMetrics).mockResolvedValue(FULL_REPORT);
    vi.mocked(listShaclFailures).mockResolvedValue([]);
    vi.mocked(listStaleTypes).mockResolvedValue([
      {
        workspace_id: "ws-1",
        type_id: "Customer",
        type_kind: "node",
        last_used_at: null,
        days_since_last_use: 240,
      },
    ]);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("Customer")).toBeInTheDocument(),
    );
    expect(screen.getByText("never")).toBeInTheDocument();
    expect(screen.getByText("240")).toBeInTheDocument();
  });
});

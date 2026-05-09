import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import messages from "../../../../../messages/en.json";
import { RunComparisonAggregate } from "../run-comparison-aggregate";
import type { components } from "@/types/api.generated";

type Aggregate = components["schemas"]["RetrievalComparisonAggregate"];

function row(overrides: Partial<Aggregate>): Aggregate {
  return {
    surface: "verified_query",
    axis: "recall_at_k",
    paired_case_count: 10,
    hybrid_mean: 0.7,
    trigram_mean: 0.5,
    mean_lift: 0.2,
    win_rate_pct: 70,
    ...overrides,
  };
}

function renderTable(rows: readonly Aggregate[]) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <RunComparisonAggregate rows={rows} />
    </NextIntlClientProvider>,
  );
}

describe("RunComparisonAggregate", () => {
  it("renders nothing when rows is empty", () => {
    const { container } = renderTable([]);
    expect(container.firstChild).toBeNull();
  });

  it("renders the table when at least one row is present", () => {
    renderTable([row({})]);
    expect(screen.getByText("Run-level hybrid lift")).toBeInTheDocument();
    // 4 axes column headers
    expect(screen.getByText("Precision@K")).toBeInTheDocument();
    expect(screen.getByText("Recall@K")).toBeInTheDocument();
    expect(screen.getByText("MRR")).toBeInTheDocument();
    expect(screen.getByText("NDCG@K")).toBeInTheDocument();
  });

  it("only emits surface rows that have at least one populated cell", () => {
    renderTable([row({ surface: "verified_query", axis: "recall_at_k" })]);
    // verified_query row present
    expect(screen.getByText("Verified queries")).toBeInTheDocument();
    // community / knowledge rows omitted because every cell is empty
    expect(screen.queryByText("Community summaries")).not.toBeInTheDocument();
    expect(screen.queryByText("Knowledge entries")).not.toBeInTheDocument();
  });

  it("formats positive lift with leading + and applies success tone via the aria label", () => {
    renderTable([row({ mean_lift: 0.2, win_rate_pct: 70, paired_case_count: 10 })]);
    expect(screen.getByText("+0.200")).toBeInTheDocument();
    // win-rate sub-line
    expect(screen.getByText(/win rate 70% · 10 pairs/)).toBeInTheDocument();
    // aria-label carries the structured signal for AT readers
    expect(
      screen.getByLabelText("lift 0.200, win rate 70%, 10 paired cases"),
    ).toBeInTheDocument();
  });

  it("formats negative lift without + (the - is already in the number)", () => {
    renderTable([row({ mean_lift: -0.1, win_rate_pct: 30, paired_case_count: 5 })]);
    expect(screen.getByText("-0.100")).toBeInTheDocument();
    expect(screen.getByText(/win rate 30% · 5 pairs/)).toBeInTheDocument();
  });

  it("renders parity sigil ±0.000 when lift is within tolerance", () => {
    renderTable([row({ mean_lift: 0, win_rate_pct: 50, paired_case_count: 4 })]);
    expect(screen.getByText("±0.000")).toBeInTheDocument();
  });

  it("treats f64 round-trip noise as parity (within 1e-6 tolerance)", () => {
    renderTable([
      row({ mean_lift: 5e-7, win_rate_pct: 50, paired_case_count: 4 }),
    ]);
    expect(screen.getByText("±0.000")).toBeInTheDocument();
  });

  it("emits em-dash for cells without a paired aggregate", () => {
    renderTable([row({ surface: "verified_query", axis: "mrr" })]);
    // Other 3 axes for verified_query render em-dash
    const dashes = screen.getAllByText("—");
    expect(dashes.length).toBeGreaterThanOrEqual(3);
    // The dashes carry an aria-label for AT
    expect(
      screen.getAllByLabelText("No paired cases on this cell").length,
    ).toBeGreaterThanOrEqual(3);
  });

  it("renders surface labels in the canonical order — verified → community → knowledge", () => {
    const rows: Aggregate[] = [
      row({ surface: "knowledge_entry", axis: "recall_at_k" }),
      row({ surface: "verified_query", axis: "recall_at_k" }),
      row({ surface: "community_summary", axis: "recall_at_k" }),
    ];
    renderTable(rows);
    const tableBody = screen.getByRole("table");
    const rowOrder = Array.from(tableBody.querySelectorAll("tbody th")).map(
      (n) => n.textContent,
    );
    expect(rowOrder).toEqual([
      "Verified queries",
      "Community summaries",
      "Knowledge entries",
    ]);
  });
});

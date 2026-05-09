import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import messages from "../../../../../messages/en.json";
import { RetrievalComparisonView } from "../retrieval-comparison-view";
import type { components } from "@/types/api.generated";

type EvaluationActual = components["schemas"]["EvaluationActual"];

function renderView(
  actual: EvaluationActual,
  expectedIds: readonly string[] = [],
) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <RetrievalComparisonView actual={actual} expectedIds={expectedIds} />
    </NextIntlClientProvider>,
  );
}

function makeComparison(overrides?: {
  hybridScores?: Partial<components["schemas"]["RetrievalMetrics"]>;
  trigramScores?: Partial<components["schemas"]["RetrievalMetrics"]>;
  hybridIds?: string[];
  trigramIds?: string[];
}): EvaluationActual {
  const defaults: components["schemas"]["RetrievalMetrics"] = {
    k: 5,
    topk_hit_count: 0,
    expected_count: 0,
    precision_at_k: 0,
    recall_at_k: 0,
    mrr: 0,
    ndcg_at_k: 0,
  };
  const hybrid_ids = overrides?.hybridIds ?? ["hit-1", "miss-1"];
  const trigram_ids = overrides?.trigramIds ?? ["miss-2", "hit-1"];
  return {
    kind: "retrieval_comparison",
    surface: "verified_query",
    hybrid: {
      anchor_ids: hybrid_ids,
      hits: hybrid_ids.map((id, i) => ({
        entity_kind: "VerifiedQuery",
        logical_id: id,
        doc: `doc-${id}`,
        score: 1 - i * 0.1,
      })),
      metrics: { ...defaults, ...overrides?.hybridScores },
    },
    trigram: {
      anchor_ids: trigram_ids,
      hits: trigram_ids.map((id, i) => ({
        entity_kind: "VerifiedQuery",
        logical_id: id,
        doc: `doc-${id}`,
        score: 1 - i * 0.1,
      })),
      metrics: { ...defaults, ...overrides?.trigramScores },
    },
  };
}

describe("RetrievalComparisonView", () => {
  it("renders nothing for non-comparison actual shapes", () => {
    const { container } = renderView({
      kind: "explanation",
      content: "answer",
      model: "claude",
    } as EvaluationActual);
    expect(container.firstChild).toBeNull();
  });

  it("renders the four canonical IR axes with both legs side-by-side", () => {
    renderView(
      makeComparison({
        hybridScores: {
          precision_at_k: 0.8,
          recall_at_k: 0.7,
          mrr: 0.6,
          ndcg_at_k: 0.75,
        },
        trigramScores: {
          precision_at_k: 0.5,
          recall_at_k: 0.4,
          mrr: 0.3,
          ndcg_at_k: 0.45,
        },
      }),
    );
    // Axis labels present
    expect(screen.getByText(/Precision@K/)).toBeInTheDocument();
    expect(screen.getByText(/Recall@K/)).toBeInTheDocument();
    expect(screen.getByText("MRR")).toBeInTheDocument();
    expect(screen.getByText(/NDCG@K/)).toBeInTheDocument();
    // Both legs' scores rendered
    expect(screen.getByText("0.800")).toBeInTheDocument();
    expect(screen.getByText("0.500")).toBeInTheDocument();
  });

  it("computes positive lift when hybrid wins and shows + sign", () => {
    renderView(
      makeComparison({
        hybridScores: { precision_at_k: 0.7 },
        trigramScores: { precision_at_k: 0.4 },
      }),
    );
    // 0.7 - 0.4 = +0.300
    expect(screen.getByText("+0.300")).toBeInTheDocument();
  });

  it("computes negative lift when trigram wins (no + sign)", () => {
    renderView(
      makeComparison({
        hybridScores: { recall_at_k: 0.3 },
        trigramScores: { recall_at_k: 0.6 },
      }),
    );
    // 0.3 - 0.6 = -0.300 (no leading +)
    expect(screen.getByText("-0.300")).toBeInTheDocument();
  });

  it("renders parity tone when scores match within tolerance", () => {
    renderView(
      makeComparison({
        // Differentiate every axis so MRR is the only parity row;
        // others carry distinct lift values that don't collide.
        hybridScores: {
          precision_at_k: 0.6,
          recall_at_k: 0.55,
          mrr: 0.5,
          ndcg_at_k: 0.65,
        },
        trigramScores: {
          precision_at_k: 0.4,
          recall_at_k: 0.35,
          mrr: 0.5,
          ndcg_at_k: 0.45,
        },
      }),
    );
    // Parity displays the ± sigil to distinguish from a `+0.000` win.
    expect(screen.getByText("±0.000")).toBeInTheDocument();
  });

  it("renders gold-standard badge for hits that match expected_ids", () => {
    renderView(makeComparison(), ["hit-1"]);
    // Both legs contain "hit-1" → 2 gold badges total
    const badges = screen.getAllByText("gold");
    expect(badges).toHaveLength(2);
  });

  it("renders empty leg message when no hits returned", () => {
    renderView(
      makeComparison({
        hybridIds: [],
        trigramIds: ["only-trigram"],
      }),
    );
    expect(screen.getByText(/No results returned/)).toBeInTheDocument();
  });

  it("emits the surface tag on the header", () => {
    renderView(makeComparison());
    expect(screen.getByText(/Surface · verified_query/)).toBeInTheDocument();
  });

  it("renders ranked positions starting at 1", () => {
    renderView(
      makeComparison({
        hybridIds: ["a", "b", "c"],
        trigramIds: ["x", "y"],
      }),
    );
    // Both legs have at least one row at position 1
    const positions = screen.getAllByText("1");
    // hybrid + trigram = 2 first-position labels
    expect(positions.length).toBeGreaterThanOrEqual(2);
  });

  it("the lift on a parity row carries the `lift` aria-label so AT can read it", () => {
    renderView(
      makeComparison({
        hybridScores: {
          precision_at_k: 0.7,
          recall_at_k: 0.6,
          mrr: 0.4,
          ndcg_at_k: 0.5,
        },
        trigramScores: {
          precision_at_k: 0.3,
          recall_at_k: 0.2,
          mrr: 0.1,
          ndcg_at_k: 0.5,
        },
      }),
    );
    // Only NDCG@K is at parity → exactly one `lift 0.000` aria-label.
    const parityCell = screen.getByLabelText(/lift 0\.000/);
    expect(parityCell).toBeInTheDocument();
  });
});

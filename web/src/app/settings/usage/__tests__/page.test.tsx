import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import UsageSettingsPage from "@/app/settings/usage/page";
import { request } from "@/lib/api/client";

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <UsageSettingsPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("UsageSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (request as ReturnType<typeof vi.fn>).mockReset();
  });

  it("aggregates totals across rows into the three summary cards", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      {
        resource_type: "chat",
        total_input_tokens: 1_200,
        total_output_tokens: 3_800,
        total_cost_usd: 0.012,
        request_count: 42,
      },
      {
        resource_type: "ontology_design",
        total_input_tokens: 5_500,
        total_output_tokens: 9_500,
        total_cost_usd: 0.079,
        request_count: 18,
      },
    ]);
    renderPage();
    // Tokens: 1200+3800+5500+9500 = 20000 → formatTokens → "20.0K".
    await waitFor(() => expect(screen.getByText("20.0K")).toBeInTheDocument());
    // Requests total.
    expect(screen.getByText("60")).toBeInTheDocument();
    // Cost — 0.091, rendered with 4 decimals ("$0.0910").
    expect(screen.getByText("$0.0910")).toBeInTheDocument();
  });

  it("formatTokens uses the `M` suffix past one million", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      {
        resource_type: "chat",
        total_input_tokens: 1_200_000,
        total_output_tokens: 300_000,
        total_cost_usd: 1.5,
        request_count: 100,
      },
    ]);
    renderPage();
    // 1_500_000 tokens → "1.5M".
    await waitFor(() => expect(screen.getByText("1.5M")).toBeInTheDocument());
  });

  it("renders the empty-state label when the server returns zero rows", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/No usage data for the selected period/),
      ).toBeInTheDocument(),
    );
  });
});

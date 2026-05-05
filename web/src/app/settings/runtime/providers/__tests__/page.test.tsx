import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

vi.mock("@/lib/api", async () => {
  const actual = await vi.importActual<Record<string, unknown>>("@/lib/api");
  return {
    ...actual,
    getHealth: vi.fn(),
  };
});

import ProvidersPage from "@/app/settings/runtime/providers/page";
import { getHealth } from "@/lib/api";

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <ProvidersPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("ProvidersPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(getHealth).mockReset();
  });

  it("renders service banner + version when health is ok", async () => {
    vi.mocked(getHealth).mockResolvedValueOnce({
      status: "ok",
      service: "ontosyx",
      version: "0.1.0",
      components: {
        llm: { status: "ok", provider: "anthropic", model: "claude-sonnet-4-6" },
        postgres: { status: "ok" },
        graph: { status: "ok", kind: "neo4j" },
      },
    } as unknown as Awaited<ReturnType<typeof getHealth>>);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(/ontosyx v0\.1\.0/)).toBeInTheDocument(),
    );
    // The overall status pill reads "Healthy" when status === "ok".
    expect(screen.getAllByText("Healthy").length).toBeGreaterThan(0);
  });

  it("surfaces a localised ErrorState with retry when getHealth throws", async () => {
    // The page renders a generic localised `ErrorState` rather than
    // surfacing `error.message` verbatim — raw API messages are in
    // English and can leak server internals to non-admin users. Admin
    // diagnostics still go through the browser console and the
    // `Retry` button drives `query.refetch()` to recover.
    vi.mocked(getHealth).mockRejectedValueOnce(new Error("upstream down"));
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Could not load data")).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: /Retry/i })).toBeInTheDocument();
  });

  it("renders the LLM + postgres + graph sub-sections once health resolves", async () => {
    vi.mocked(getHealth).mockResolvedValueOnce({
      status: "degraded",
      service: "ontosyx",
      version: "0.2.0",
      components: {
        llm: { status: "ok", provider: "openai", model: "gpt-4o" },
        postgres: { status: "ok" },
        graph: { status: "unavailable", kind: "neo4j" },
      },
    } as unknown as Awaited<ReturnType<typeof getHealth>>);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("LLM Provider")).toBeInTheDocument(),
    );
    expect(screen.getByText("PostgreSQL")).toBeInTheDocument();
    expect(screen.getByText("Graph Database")).toBeInTheDocument();
  });
});

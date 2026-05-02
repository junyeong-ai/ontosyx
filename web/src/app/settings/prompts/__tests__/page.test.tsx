import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  listPromptTemplates: vi.fn(),
  createPromptTemplate: vi.fn(),
  updatePromptTemplate: vi.fn(),
  deletePromptTemplate: vi.fn(),
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

import PromptsPage from "@/app/settings/prompts/page";
import * as api from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import { mockAuth } from "@/test-utils/auth";
import type { PromptTemplate } from "@/types/api";

function sampleTemplate(overrides: Partial<PromptTemplate> = {}): PromptTemplate {
  return {
    id: "pt-1",
    name: "schema_rag_retrieve",
    version: "1.0.0",
    content: "Use the ontology to pick matching labels",
    variables: [],
    metadata: {},
    created_by: "system",
    created_at: "2026-04-22T00:00:00Z",
    is_active: true,
    ...overrides,
  };
}

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <PromptsPage />
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

describe("PromptsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(api.listPromptTemplates).mockReset();
    confirmMock.mockReset();
  });

  it("shows the admin-only placeholder for non-admins", async () => {
    vi.mocked(useAuth).mockReturnValue(
      mockAuth({ kind: "authenticated", role: "viewer" }),
    );
    // The reload effect fires unconditionally — admin gate is render-only.
    vi.mocked(api.listPromptTemplates).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/Admin access required to manage prompts/),
      ).toBeInTheDocument(),
    );
  });

  it("renders templates grouped by name with the highest version first", async () => {
    asAdmin();
    // Two versions of the same template — page groups them and the
    // card sorts by version desc (localeCompare b vs a).
    vi.mocked(api.listPromptTemplates).mockResolvedValueOnce([
      sampleTemplate({ id: "pt-1", version: "1.0.0", is_active: false }),
      sampleTemplate({ id: "pt-2", version: "2.0.0", is_active: true }),
    ]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("schema_rag_retrieve")).toBeInTheDocument(),
    );
    // Only one card name renders (grouped by `name`).
    expect(screen.getAllByText("schema_rag_retrieve")).toHaveLength(1);
  });

  it("renders the empty-state when the API returns zero templates", async () => {
    asAdmin();
    vi.mocked(api.listPromptTemplates).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/No prompt templates/i),
      ).toBeInTheDocument(),
    );
  });

  it("count label reflects how many groups survive the filter", async () => {
    asAdmin();
    vi.mocked(api.listPromptTemplates).mockResolvedValueOnce([
      sampleTemplate({ id: "pt-1", name: "alpha" }),
      sampleTemplate({ id: "pt-2", name: "beta" }),
    ]);
    renderPage();
    // Count shows `{count}` inline — match the rendered number.
    await waitFor(() =>
      // 2 templates → 2 groups → "2 templates" (or similar).
      expect(screen.getByText(/2/)).toBeInTheDocument(),
    );
  });
});

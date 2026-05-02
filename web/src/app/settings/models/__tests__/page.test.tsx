import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
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

import ModelsSettingsPage from "@/app/settings/models/page";
import { request } from "@/lib/api/client";
import { toast } from "sonner";

const CONFIG_ENABLED = {
  id: "cfg-1",
  name: "Opus prod",
  provider: "anthropic",
  model_id: "claude-opus-4-7",
  max_tokens: 4096,
  temperature: 0.2,
  timeout_secs: 120,
  cost_per_1m_input: 15,
  cost_per_1m_output: 75,
  daily_budget_usd: 100,
  priority: 0,
  enabled: true,
  api_key_env: "ANTHROPIC_API_KEY",
  region: null,
  base_url: null,
};

const CONFIG_DISABLED = {
  ...CONFIG_ENABLED,
  id: "cfg-2",
  name: "Sonnet backup",
  model_id: "claude-sonnet-4-6",
  enabled: false,
};

const RULE = {
  id: "rule-1",
  operation: "design_ontology",
  model_config_id: "cfg-1",
  priority: 100,
  enabled: true,
};

function setMocks(
  configs: typeof CONFIG_ENABLED[] = [],
  rules: typeof RULE[] = [],
) {
  vi.mocked(request).mockImplementation((url: string) => {
    if (url === "/models/configs") return Promise.resolve(configs);
    if (url === "/models/routing-rules") return Promise.resolve(rules);
    return Promise.resolve(undefined);
  });
}

describe("ModelsSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(request).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("renders both config rows + the routing rule once initial data resolves", async () => {
    setMocks([CONFIG_ENABLED, CONFIG_DISABLED], [RULE]);
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );
    expect(screen.getByText("Sonnet backup")).toBeInTheDocument();
    // Rule row uses the human config name (not the uuid).
    expect(screen.getByText("design_ontology")).toBeInTheDocument();
  });

  it("computes the three summary cards from the config + rule lists", async () => {
    setMocks([CONFIG_ENABLED, CONFIG_DISABLED], [RULE]);
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );
    // 1 enabled + 1 disabled + 1 routing rule. "Enabled" + "Disabled"
    // appear both in the summary cards (text-xs label) and the table
    // column header — pick the small-text label which sits next to the
    // count number in the same card.
    const enabledLabels = screen.getAllByText("Enabled");
    const disabledLabels = screen.getAllByText("Disabled");
    const ruleLabels = screen.getAllByText("Routing Rules");
    expect(
      enabledLabels.find((n) => n.previousSibling?.textContent === "1"),
    ).toBeTruthy();
    expect(
      disabledLabels.find((n) => n.previousSibling?.textContent === "1"),
    ).toBeTruthy();
    expect(
      ruleLabels.find((n) => n.previousSibling?.textContent === "1"),
    ).toBeTruthy();
  });

  it("Delete with confirm=true calls DELETE and shows the success toast", async () => {
    setMocks([CONFIG_ENABLED], []);
    confirmMock.mockResolvedValueOnce(true);
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );

    // Two Delete buttons can exist (config + rule); since rules=[], only one.
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(request).toHaveBeenCalledWith(
        "/models/configs/cfg-1",
        expect.objectContaining({ method: "DELETE" }),
      ),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Model config deleted"),
    );
  });

  it("Test config posts to /models/test and surfaces the latency in a success toast", async () => {
    vi.mocked(request).mockImplementation((url: string) => {
      if (url === "/models/configs")
        return Promise.resolve([CONFIG_ENABLED]);
      if (url === "/models/routing-rules") return Promise.resolve([]);
      if (url === "/models/test") {
        return Promise.resolve({
          success: true,
          latency_ms: 240,
          error: null,
        });
      }
      return Promise.resolve(undefined);
    });
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Test$/ }));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Model responded in 240ms"),
    );
    expect(request).toHaveBeenCalledWith(
      "/models/test",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ model_config_id: "cfg-1" }),
      }),
    );
  });
});

import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { GlossaryBindingPanel } from "@/components/settings/glossary/binding-panel";
import * as bindingApi from "@/lib/api/binding-suggestions";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function renderPanel() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <GlossaryBindingPanel ontologyId="ont-1" expectedVersion={2} />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  return render(ui);
}

describe("GlossaryBindingPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("Score candidates forwards term + aliases + description + term_id", async () => {
    const suggest = vi
      .spyOn(bindingApi, "suggestGlossaryBindings")
      .mockResolvedValue({ ontology_id: "ont-1", candidates: [] });

    renderPanel();
    fireEvent.change(screen.getByLabelText(/^Term\s*\*?$/), {
      target: { value: "VIP tier" },
    });
    fireEvent.change(screen.getByLabelText(/^Term id/), {
      target: { value: "g-vip" },
    });
    fireEvent.change(screen.getByLabelText(/^Aliases/), {
      target: { value: "premium, loyalty" },
    });
    fireEvent.change(screen.getByLabelText(/^Description/), {
      target: { value: "Top-tier customers." },
    });
    fireEvent.click(screen.getByRole("button", { name: /Score candidates/ }));

    await waitFor(() => expect(suggest).toHaveBeenCalled());
    expect(suggest).toHaveBeenCalledWith("ont-1", {
      term: "VIP tier",
      aliases: ["premium", "loyalty"],
      description: "Top-tier customers.",
      term_id: "g-vip",
    });
  });

  it("renders candidate rows with owner, property, score percent", async () => {
    vi.spyOn(bindingApi, "suggestGlossaryBindings").mockResolvedValue({
      ontology_id: "ont-1",
      candidates: [
        {
          owner_kind: "node",
          owner_type_id: "Customer",
          owner_label: "Customer",
          property_id: "tier",
          property_name: "tier",
          score: 0.85,
          signals: [{ kind: "canonical_name" }],
        },
      ],
    });
    renderPanel();
    fireEvent.change(screen.getByLabelText(/^Term\s*\*?$/), {
      target: { value: "VIP tier" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Score candidates/ }));

    await waitFor(() => screen.getByText("85%"));
    expect(screen.getByText("Customer")).toBeDefined();
    expect(screen.getByText("tier")).toBeDefined();
  });

  it("batch bind fires /edits with one bind_property_to_term per selected row", async () => {
    vi.spyOn(bindingApi, "suggestGlossaryBindings").mockResolvedValue({
      ontology_id: "ont-1",
      candidates: [
        {
          owner_kind: "node",
          owner_type_id: "Customer",
          owner_label: "Customer",
          property_id: "tier",
          property_name: "tier",
          score: 0.85,
          signals: [],
        },
        {
          owner_kind: "edge",
          owner_type_id: "PLACED",
          owner_label: "PLACED",
          property_id: "channel",
          property_name: "channel",
          score: 0.72,
          signals: [],
        },
      ],
    });
    const apply = vi
      .spyOn(bindingApi, "applyOntologyEdits")
      .mockResolvedValue({
        new_version: 3,
        new_version_id: "vid-1",
        parent_version_id: "vid-0",
        applied_operations: 2,
        committed_at: "2026-04-22T00:00:00Z",
      });

    renderPanel();
    fireEvent.change(screen.getByLabelText(/^Term\s*\*?$/), {
      target: { value: "VIP tier" },
    });
    fireEvent.change(screen.getByLabelText(/^Term id/), {
      target: { value: "g-vip" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Score candidates/ }));
    await waitFor(() => screen.getByText("tier"));

    // Tick both rows.
    const boxes = screen.getAllByRole("checkbox");
    fireEvent.click(boxes[0]);
    fireEvent.click(boxes[1]);

    fireEvent.click(screen.getByRole("button", { name: /Bind 2 selected/ }));

    await waitFor(() => expect(apply).toHaveBeenCalled());
    const [, body] = apply.mock.calls[0] ?? [];
    expect(body?.expected_version).toBe(2);
    expect(body?.operations).toEqual([
      {
        op: "bind_property_to_term",
        owner: { kind: "node", type_id: "Customer" },
        property_id: "tier",
        glossary_term_id: "g-vip",
      },
      {
        op: "bind_property_to_term",
        owner: { kind: "edge", type_id: "PLACED" },
        property_id: "channel",
        glossary_term_id: "g-vip",
      },
    ]);
  });
});

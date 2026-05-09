import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { ConceptBindingPanel } from "@/components/glossary/concept-binding-panel";
import * as bindingApi from "@/lib/api/binding-suggestions";
import * as editOps from "@/lib/api/edit-ops";

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

const FIXTURE_TERM = {
  term_id: "g-vip",
  concept_id: "c-vip",
  term: { default: "VIP tier" },
  aliases: [{ default: "premium" }, { default: "loyalty" }],
  description: { default: "Top-tier customers." },
};

function renderPanel(term = FIXTURE_TERM) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <ConceptBindingPanel
          ontologyId="ont-1"
          expectedVersion={2}
          term={term}
        />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  return render(ui);
}

describe("ConceptBindingPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("auto-scores on mount with term + aliases + description + term_id", async () => {
    const suggest = vi
      .spyOn(bindingApi, "suggestConceptPropertyBindings")
      .mockResolvedValue({ ontology_id: "ont-1", candidates: [] });

    renderPanel();

    await waitFor(() => expect(suggest).toHaveBeenCalled());
    expect(suggest).toHaveBeenCalledWith("workspace", {
      term: { default: "VIP tier" },
      aliases: [{ default: "premium" }, { default: "loyalty" }],
      description: { default: "Top-tier customers." },
      term_id: "g-vip",
    });
  });

  it("renders candidate rows with owner, property, score percent", async () => {
    vi.spyOn(bindingApi, "suggestConceptPropertyBindings").mockResolvedValue({
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

    await waitFor(() => screen.getByText("85%"));
    expect(screen.getByText("Customer")).toBeDefined();
    expect(screen.getByText("tier")).toBeDefined();
  });

  it("batch bind fires /edits with one bind_property per selected row", async () => {
    vi.spyOn(bindingApi, "suggestConceptPropertyBindings").mockResolvedValue({
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
      .spyOn(editOps, "submitOntologyEdits")
      .mockResolvedValue({
        new_version: 3,
        new_version_id: "vid-1",
        parent_version_id: "vid-0",
        applied_operations: 2,
        committed_at: "2026-04-22T00:00:00Z",
      });

    renderPanel();
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
        op: "bind_property",
        owner: { kind: "node", type_id: "Customer" },
        property_id: "tier",
        binding: { kind: "concept", id: "c-vip" },
      },
      {
        op: "bind_property",
        owner: { kind: "edge", type_id: "PLACED" },
        property_id: "channel",
        binding: { kind: "concept", id: "c-vip" },
      },
    ]);
  });
});

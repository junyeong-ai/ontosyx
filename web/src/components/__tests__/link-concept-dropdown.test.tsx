import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { LinkConceptDropdown } from "@/components/workbench/inspector/link-concept-dropdown";
import * as bindingApi from "@/lib/api/binding-suggestions";
import * as editOps from "@/lib/api/edit-ops";

// Stub the API surface so the component exercises its full state
// machine (loading / populated / pick / unbind) without network.
// `vi.spyOn` keeps the real exports importable, critical for the
// typed variables and for other tests that import from the same file.
vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function renderWithProviders(ui: ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>{ui}</QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

const defaultProps = {
  ontologyId: "ont-1",
  expectedVersion: 3,
  ownerKind: "node" as const,
  ownerTypeId: "Customer",
  propertyId: "p-tier",
};

describe("LinkConceptDropdown", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the unbind pill when a concept binding is present", () => {
    renderWithProviders(
      <LinkConceptDropdown {...defaultProps} boundBinding={{ kind: "concept", id: "c-vip" }} />,
    );
    expect(
      screen.getByRole("button", { name: /unbind from c-vip/i }),
    ).toBeDefined();
    // The suggest-concepts mutation must NOT fire when already bound.
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("fetches suggestions when opened and renders candidate scores", async () => {
    const suggest = vi
      .spyOn(bindingApi, "suggestConceptsForProperty")
      .mockResolvedValue({
        ontology_id: "ont-1",
        candidates: [
          { term_id: "vip", concept_id: "c-vip", term: { default: "VIP" }, score: 0.92, signals: [] },
          { term_id: "tier", concept_id: "c-tier", term: { default: "Customer Tier" }, score: 0.77, signals: [] },
        ],
      });

    renderWithProviders(<LinkConceptDropdown {...defaultProps} />);

    // Open the dropdown.
    fireEvent.click(
      screen.getByRole("button", {
        name: /link this property to a concept/i,
      }),
    );

    // The candidate rows appear after the mutation resolves.
    await waitFor(() => {
      expect(screen.getByText("VIP")).toBeDefined();
      expect(screen.getByText("Customer Tier")).toBeDefined();
    });
    // Scores render as rounded percentages.
    expect(screen.getByText("92%")).toBeDefined();
    expect(screen.getByText("77%")).toBeDefined();
    expect(suggest).toHaveBeenCalledWith(
      "workspace",
      "node",
      "Customer",
      "p-tier",
      { max_results: 3 },
    );
  });

  it("surfaces an empty-state when the API returns no candidates", async () => {
    vi.spyOn(bindingApi, "suggestConceptsForProperty").mockResolvedValue({
      ontology_id: "ont-1",
      candidates: [],
    });

    renderWithProviders(<LinkConceptDropdown {...defaultProps} />);
    fireEvent.click(
      screen.getByRole("button", {
        name: /link this property to a concept/i,
      }),
    );

    await waitFor(() => {
      expect(
        screen.getByText(/no matching concepts/i),
      ).toBeDefined();
    });
  });

  it("clicking a candidate fires a bind_property edit with the concept target", async () => {
    vi.spyOn(bindingApi, "suggestConceptsForProperty").mockResolvedValue({
        ontology_id: "ont-1",
        candidates: [
          { term_id: "vip", concept_id: "c-vip", term: { default: "VIP" }, score: 0.92, signals: [] },
        ],
      });
    const apply = vi
      .spyOn(editOps, "submitOntologyEdits")
      .mockResolvedValue({
        new_version: 4,
        new_version_id: "vid-1",
        parent_version_id: "vid-0",
        applied_operations: 1,
        committed_at: "2026-04-22T00:00:00Z",
      });

    renderWithProviders(<LinkConceptDropdown {...defaultProps} />);
    fireEvent.click(
      screen.getByRole("button", {
        name: /link this property to a concept/i,
      }),
    );
    await waitFor(() => screen.getByText("VIP"));
    fireEvent.click(screen.getByRole("option", { name: /VIP/ }));

    await waitFor(() => {
      expect(apply).toHaveBeenCalled();
    });
    const [, body] = apply.mock.calls[0] ?? [];
    expect(body?.expected_version).toBe(3);
    expect(body?.operations).toEqual([
      {
        op: "bind_property",
        owner: { kind: "node", type_id: "Customer" },
        property_id: "p-tier",
        binding: { kind: "concept", id: "c-vip" },
      },
    ]);
  });

  it("clicking the bound pill fires an unbind_property edit", async () => {
    const apply = vi
      .spyOn(editOps, "submitOntologyEdits")
      .mockResolvedValue({
        new_version: 4,
        new_version_id: "vid-1",
        parent_version_id: "vid-0",
        applied_operations: 1,
        committed_at: "2026-04-22T00:00:00Z",
      });

    renderWithProviders(
      <LinkConceptDropdown {...defaultProps} boundBinding={{ kind: "concept", id: "c-vip" }} />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: /unbind from c-vip/i }),
    );

    await waitFor(() => {
      expect(apply).toHaveBeenCalled();
    });
    const [, body] = apply.mock.calls[0] ?? [];
    expect(body?.operations[0]).toMatchObject({
      op: "unbind_property",
      target: { kind: "concept", id: "c-vip" },
    });
  });
});

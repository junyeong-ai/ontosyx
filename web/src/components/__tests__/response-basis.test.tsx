import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../messages/en.json";
import { ResponseBasis } from "@/components/dashboard/widgets/response-basis";
import type {
  OntologyDetail,
  QueryDiagnostic,
  QueryProvenance,
} from "@/types/api";

// `useWorkspaceOntology` hits a TanStack Query + network fetch in real life.
// Mock it so each test declares the hook's return value explicitly —
// keeps the tests focused on the resolution logic without a
// QueryClientProvider or MSW setup.
const useWorkspaceOntologyMock = vi.fn();
vi.mock("@/hooks/api/use-workspace-ontology", () => ({
  useWorkspaceOntology: (...args: unknown[]) => useWorkspaceOntologyMock(...args),
}));

// `useLocaleChain` is also a TanStack-backed hook (workspace fetch).
// Tests don't exercise the chain itself, so a fixed boot fallback
// keeps the resolution path deterministic.
vi.mock("@/hooks/use-locale-chain", () => ({
  useLocaleChain: () => ["en"],
}));

function renderWithIntl(ui: React.ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

/** Minimal `OntologyDetail` shape with two node types + one edge. */
function ontologyDetailFixture(): OntologyDetail {
  return {
    id: "ont-1",
    lineage_id: "lineage-1",
    name: "Fixture",
    description: { default: "fixture" },
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ontology_ir: {
      id: "lineage-1",
      name: "Fixture",
      description: { default: "" },
      version: { number: 1 },
      node_types: [
        {
          id: "nt_customer",
          label: "Customer",
          description: { default: "Buyers of products" },
          properties: [],
        },
        {
          id: "nt_product",
          label: "Product",
          description: { default: "" },
          properties: [],
        },
      ],
      edge_types: [
        {
          id: "et_purchased",
          label: "PURCHASED",
          description: { default: "Purchase event linking customers to products" },
          source_node_id: "nt_customer",
          target_node_id: "nt_product",
          properties: [],
        },
      ],
    },
  };
}

describe("ResponseBasis", () => {
  beforeEach(() => {
    useWorkspaceOntologyMock.mockReset();
    // Default: no ontology loaded.
    useWorkspaceOntologyMock.mockReturnValue({ data: undefined });
  });

  it("renders nothing when provenance is absent", () => {
    const { container } = renderWithIntl(<ResponseBasis provenance={null} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing when every field is empty", () => {
    const empty: QueryProvenance = {
      source_ids: [],
      type_ids: [],
    };
    const { container } = renderWithIntl(<ResponseBasis provenance={empty} />);
    expect(container.firstChild).toBeNull();
  });

  it("shows ontology id + version + as_of when present", () => {
    const prov: QueryProvenance = {
      ontology_id: "11111111-1111-1111-1111-111111111111",
      ontology_version: "3",
      as_of: "2026-03-15T00:00:00Z",
    };
    renderWithIntl(<ResponseBasis provenance={prov} />);
    expect(screen.getByText("11111111-1111-1111-1111-111111111111")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("2026-03-15T00:00:00Z")).toBeInTheDocument();
  });

  it("renders sources as pills and shows raw type ids when ontology is unavailable", () => {
    const prov: QueryProvenance = {
      source_ids: ["pg-main", "csv-orders"],
      type_ids: ["nt_customer", "et_purchased"],
    };
    renderWithIntl(<ResponseBasis provenance={prov} />);
    expect(screen.getByText("pg-main")).toBeInTheDocument();
    expect(screen.getByText("csv-orders")).toBeInTheDocument();
    // No ontology_detail fixture in this render — ids render verbatim.
    expect(screen.getByText("nt_customer")).toBeInTheDocument();
    expect(screen.getByText("et_purchased")).toBeInTheDocument();
  });

  it("renders resolved labels with tooltip when ontology is available", () => {
    useWorkspaceOntologyMock.mockReturnValue({ data: ontologyDetailFixture() });
    const prov: QueryProvenance = {
      ontology_id: "ont-1",
      type_ids: ["nt_customer", "et_purchased"],
    };
    renderWithIntl(<ResponseBasis provenance={prov} />);

    // Labels take priority over raw ids.
    expect(screen.getByText("Customer")).toBeInTheDocument();
    expect(screen.getByText("PURCHASED")).toBeInTheDocument();
    expect(screen.queryByText("nt_customer")).not.toBeInTheDocument();

    // Tooltip carries the full (id · description) context.
    const node = screen.getByText("Customer");
    expect(node).toHaveAttribute("title", "nt_customer · Buyers of products");

    const edge = screen.getByText("PURCHASED");
    expect(edge).toHaveAttribute(
      "title",
      "et_purchased · Purchase event linking customers to products",
    );
  });

  it("falls back to the raw id when a type_id does not match any type", () => {
    useWorkspaceOntologyMock.mockReturnValue({ data: ontologyDetailFixture() });
    const prov: QueryProvenance = {
      ontology_id: "ont-1",
      type_ids: ["nt_customer", "nt_missing"],
    };
    renderWithIntl(<ResponseBasis provenance={prov} />);
    expect(screen.getByText("Customer")).toBeInTheDocument();
    // Unresolved id renders verbatim (mono-font fallback).
    expect(screen.getByText("nt_missing")).toBeInTheDocument();
  });

  it("renders the filter summary verbatim", () => {
    const prov: QueryProvenance = {
      filter_summary: 'status = "ACTIVE"; (a.age > 30)',
    };
    renderWithIntl(<ResponseBasis provenance={prov} />);
    expect(
      screen.getByText(/status = "ACTIVE"; \(a\.age > 30\)/),
    ).toBeInTheDocument();
  });

  it("omits the types row when type_ids is empty even if ontology is loaded", () => {
    useWorkspaceOntologyMock.mockReturnValue({ data: ontologyDetailFixture() });
    const prov: QueryProvenance = {
      ontology_id: "ont-1",
      ontology_version: "1",
    };
    renderWithIntl(<ResponseBasis provenance={prov} />);
    expect(screen.queryByText("Customer")).not.toBeInTheDocument();
    expect(screen.queryByText("PURCHASED")).not.toBeInTheDocument();
  });

  it("renders the warnings list when provided", () => {
    const warnings: QueryDiagnostic[] = [
      {
        validator: "complexity",
        level: "warning",
        message: {
          code: "runtime.cypher.complexity.unbounded_var_length",
          message: "variable-length relationship has no upper bound",
        },
      },
      {
        validator: "semantic-guard",
        level: "error",
        message: {
          code: "runtime.cypher.semantic_guard.tautological_where",
          message: "destructive operation is gated only by a tautological WHERE",
        },
      },
    ];
    renderWithIntl(
      <ResponseBasis
        provenance={{ ontology_id: "ont-1" }}
        warnings={warnings}
      />,
    );
    // Renderings come from the FE i18n catalogue keyed by `code`.
    // The case-insensitive regexes assert the substantive phrase
    // each catalogue entry surfaces, not the exact wording — so a
    // catalogue-only edit (e.g. tightening copy) doesn't churn this
    // test.
    expect(
      screen.getByText(/variable-length relationship has no upper bound/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/tautological WHERE predicate/i),
    ).toBeInTheDocument();
    // Structured access — the UI renders validator + level as a header
    expect(screen.getByText(/complexity warning:/)).toBeInTheDocument();
    expect(screen.getByText(/semantic-guard error:/)).toBeInTheDocument();
  });

  it("shows warnings even when provenance is empty", () => {
    renderWithIntl(
      <ResponseBasis
        provenance={null}
        warnings={[
          {
            validator: "complexity",
            level: "warning",
            // Code without a catalogue entry — `resolveDiagnostic`
            // falls back to the structured English `message`. Tests
            // the missing-key fallback path.
            message: {
              code: "test.no_catalogue_entry",
              message: "ad-hoc-message-for-this-test",
            },
          },
        ]}
      />,
    );
    expect(
      screen.getByText(/ad-hoc-message-for-this-test/),
    ).toBeInTheDocument();
  });

  it("filters empty-message warnings before checking emptiness", () => {
    const { container } = renderWithIntl(
      <ResponseBasis
        provenance={null}
        warnings={[
          {
            validator: "complexity",
            level: "warning",
            message: { code: "test.empty", message: "" },
          },
          {
            validator: "complexity",
            level: "info",
            message: { code: "test.whitespace", message: "   " },
          },
        ]}
      />,
    );
    expect(container.firstChild).toBeNull();
  });
});

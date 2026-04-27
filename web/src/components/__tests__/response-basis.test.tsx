import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../messages/en.json";
import { ResponseBasis } from "@/components/widgets/response-basis";
import type {
  OntologyDetail,
  QueryDiagnostic,
  QueryProvenance,
} from "@/types/api";

// `useOntologyDetail` hits a TanStack Query + network fetch in real life.
// Mock it so each test declares the hook's return value explicitly —
// keeps the tests focused on the resolution logic without a
// QueryClientProvider or MSW setup.
const useOntologyDetailMock = vi.fn();
vi.mock("@/hooks/api/use-ontologies", () => ({
  useOntologyDetail: (...args: unknown[]) => useOntologyDetailMock(...args),
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
      version: 1,
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
    useOntologyDetailMock.mockReset();
    // Default: no ontology loaded.
    useOntologyDetailMock.mockReturnValue({ data: undefined });
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
    useOntologyDetailMock.mockReturnValue({ data: ontologyDetailFixture() });
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
    useOntologyDetailMock.mockReturnValue({ data: ontologyDetailFixture() });
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
    useOntologyDetailMock.mockReturnValue({ data: ontologyDetailFixture() });
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
        message: "variable-length relationship has no upper bound",
      },
      {
        validator: "semantic-guard",
        level: "error",
        message: "destructive operation is gated only by a tautological WHERE",
      },
    ];
    renderWithIntl(
      <ResponseBasis
        provenance={{ ontology_id: "ont-1" }}
        warnings={warnings}
      />,
    );
    expect(
      screen.getByText(/variable-length relationship has no upper bound/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/destructive operation is gated only by a tautological WHERE/),
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
            message: "ad-hoc-message-for-this-test",
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
          { validator: "complexity", level: "warning", message: "" },
          { validator: "complexity", level: "info", message: "   " },
        ]}
      />,
    );
    expect(container.firstChild).toBeNull();
  });
});

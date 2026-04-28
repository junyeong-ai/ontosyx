import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import enMessages from "../../../../messages/en.json";
import { LineageTree } from "@/components/ontology/lineage-tree";
import type {
  DependencyEdge,
  SchemaEntityRef,
} from "@/lib/api/dependencies";

function renderTree(ui: React.ReactElement): ReturnType<typeof render> {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

function nodeRef(id: string): SchemaEntityRef {
  return { kind: "node_type", id };
}

function edge(
  endpoint: SchemaEntityRef,
  kind: DependencyEdge["kind"],
  label = "edge label",
): DependencyEdge {
  return { endpoint, kind, label };
}

describe("LineageTree", () => {
  it("renders the empty-state copy when no edges are passed", () => {
    renderTree(<LineageTree edges={[]} direction="inbound" />);
    expect(
      screen.getByText("Nothing depends on this entity"),
    ).toBeDefined();
  });

  it("groups edges by kind and shows per-group counts", () => {
    const edges: DependencyEdge[] = [
      edge(nodeRef("Customer"), "edge_source", "source of `places`"),
      edge(nodeRef("Order"), "edge_source", "source of `pays_for`"),
      edge(nodeRef("Order"), "edge_target", "target of `places`"),
    ];
    renderTree(<LineageTree edges={edges} direction="inbound" />);
    expect(screen.getByText("Edge source")).toBeDefined();
    expect(screen.getByText("Edge target")).toBeDefined();
    // Per-group count badges
    expect(screen.getByText("2")).toBeDefined();
    expect(screen.getByText("1")).toBeDefined();
  });

  it("calls onSelect with the endpoint ref when a row is clicked", () => {
    const onSelect = vi.fn();
    const edges: DependencyEdge[] = [
      edge(nodeRef("Customer"), "edge_source"),
    ];
    renderTree(
      <LineageTree
        edges={edges}
        direction="inbound"
        onSelect={onSelect}
      />,
    );
    fireEvent.click(screen.getByText("Customer"));
    expect(onSelect).toHaveBeenCalledWith({
      kind: "node_type",
      id: "Customer",
    });
  });

  it("uses labelOf to humanize endpoint refs", () => {
    const edges: DependencyEdge[] = [
      edge(nodeRef("nt-cust-001"), "edge_source"),
    ];
    renderTree(
      <LineageTree
        edges={edges}
        direction="inbound"
        labelOf={(ref) => (ref.kind === "node_type" ? "Customer" : null)}
      />,
    );
    expect(screen.getByText("Customer")).toBeDefined();
  });

  it("shows the outbound empty-state copy when direction is outbound", () => {
    renderTree(<LineageTree edges={[]} direction="outbound" />);
    expect(
      screen.getByText("This entity has no outbound references"),
    ).toBeDefined();
  });
});

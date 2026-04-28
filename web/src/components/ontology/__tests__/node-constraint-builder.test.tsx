import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import enMessages from "../../../../messages/en.json";
import { NodeConstraintBuilder } from "@/components/ontology/node-constraint-builder";
import type { ConstraintDef, NodeTypeDef, PropertyDef } from "@/types/ontology";

vi.stubGlobal("crypto", {
  randomUUID: () => "11111111-2222-3333-4444-555555555555",
});

function renderBuilder(ui: React.ReactElement): ReturnType<typeof render> {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

function property(id: string, name: string): PropertyDef {
  return {
    id,
    name,
    property_type: { type: "string" },
    description: { default: "" },
  };
}

function fixture(constraints: ConstraintDef[] = []): NodeTypeDef {
  return {
    id: "nt-customer",
    label: "Customer",
    description: { default: "" },
    properties: [
      property("p-id", "id"),
      property("p-email", "email"),
      property("p-name", "name"),
    ],
    constraints,
  };
}

describe("NodeConstraintBuilder", () => {
  it("renders the empty-state copy when the node has no constraints", () => {
    renderBuilder(
      <NodeConstraintBuilder
        node={fixture()}
        onAdd={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(
      screen.getByText("This node type has no constraints yet."),
    ).toBeDefined();
  });

  it("renders existing constraints with their human-readable summary", () => {
    const constraints: ConstraintDef[] = [
      { id: "c-1", type: "unique", property_ids: ["p-email"] },
      { id: "c-2", type: "exists", property_id: "p-name" },
      {
        id: "c-3",
        type: "node_key",
        property_ids: ["p-id"],
      },
    ];
    renderBuilder(
      <NodeConstraintBuilder
        node={fixture(constraints)}
        onAdd={() => {}}
        onRemove={() => {}}
      />,
    );
    expect(screen.getByText("UNIQUE(email)")).toBeDefined();
    expect(screen.getByText("EXISTS(name)")).toBeDefined();
    expect(screen.getByText("NODE KEY(id)")).toBeDefined();
  });

  it("emits a UNIQUE constraint with the picked properties", () => {
    const onAdd = vi.fn();
    renderBuilder(
      <NodeConstraintBuilder
        node={fixture()}
        onAdd={onAdd}
        onRemove={() => {}}
      />,
    );
    fireEvent.click(screen.getByText("+ Add constraint"));
    fireEvent.click(screen.getByText("email"));
    fireEvent.click(screen.getByText("name"));
    fireEvent.click(screen.getByText("Add"));
    expect(onAdd).toHaveBeenCalledWith({
      id: "cd-11111111-2222-3333-4444-555555555555",
      type: "unique",
      property_ids: ["p-email", "p-name"],
    });
  });

  it("forces single-selection when kind is EXISTS", () => {
    const onAdd = vi.fn();
    renderBuilder(
      <NodeConstraintBuilder
        node={fixture()}
        onAdd={onAdd}
        onRemove={() => {}}
      />,
    );
    fireEvent.click(screen.getByText("+ Add constraint"));
    fireEvent.change(screen.getByDisplayValue("UNIQUE"), {
      target: { value: "exists" },
    });
    fireEvent.click(screen.getByText("email"));
    fireEvent.click(screen.getByText("name")); // should replace, not add
    fireEvent.click(screen.getByText("Add"));
    expect(onAdd).toHaveBeenCalledWith({
      id: "cd-11111111-2222-3333-4444-555555555555",
      type: "exists",
      property_id: "p-name",
    });
  });

  it("calls onRemove with the constraint id when its delete button is clicked", () => {
    const onRemove = vi.fn();
    const constraint: ConstraintDef = {
      id: "c-1",
      type: "unique",
      property_ids: ["p-email"],
    };
    renderBuilder(
      <NodeConstraintBuilder
        node={fixture([constraint])}
        onAdd={() => {}}
        onRemove={onRemove}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Remove constraint: UNIQUE/ }),
    );
    expect(onRemove).toHaveBeenCalledWith("c-1");
  });

  it("hides every add/remove affordance when readOnly", () => {
    const constraint: ConstraintDef = {
      id: "c-1",
      type: "unique",
      property_ids: ["p-email"],
    };
    renderBuilder(
      <NodeConstraintBuilder
        node={fixture([constraint])}
        onAdd={() => {}}
        onRemove={() => {}}
        readOnly
      />,
    );
    expect(screen.queryByText("+ Add constraint")).toBeNull();
    expect(
      screen.queryByRole("button", { name: /Remove constraint/ }),
    ).toBeNull();
  });
});

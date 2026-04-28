import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import enMessages from "../../../../messages/en.json";
import { InlineObjectMappingEditor } from "@/components/ontology/inline-object-mapping-editor";
import type { ObjectMappingDef } from "@/lib/api/edit-ops";
import type { PropertyDef } from "@/types/ontology";

function renderEditor(ui: React.ReactElement): ReturnType<typeof render> {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

const PROPERTIES: PropertyDef[] = [
  {
    id: "p-id",
    name: "id",
    property_type: { type: "string" },
    description: { default: "" },
  },
  {
    id: "p-name",
    name: "name",
    property_type: { type: "string" },
    description: { default: "" },
  },
];

function skeleton(): ObjectMappingDef {
  return {
    id: "om-test",
    node_type_id: "nt-customer",
    source_id: "src-warehouse",
    relation: "",
    property_mappings: [],
  };
}

function withRelation(relation: string): ObjectMappingDef {
  return { ...skeleton(), relation };
}

describe("InlineObjectMappingEditor", () => {
  it("emits patched relation on every keystroke", () => {
    const onChange = vi.fn();
    renderEditor(
      <InlineObjectMappingEditor
        value={skeleton()}
        properties={PROPERTIES}
        onChange={onChange}
      />,
    );
    const input = screen.getByPlaceholderText("e.g. public.customers");
    fireEvent.change(input, { target: { value: "public.customers" } });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ relation: "public.customers" }),
    );
  });

  it("adds a primary key column when committed", () => {
    const onChange = vi.fn();
    renderEditor(
      <InlineObjectMappingEditor
        value={withRelation("public.customers")}
        properties={PROPERTIES}
        onChange={onChange}
      />,
    );
    const pkInput = screen.getByPlaceholderText("+ column");
    fireEvent.change(pkInput, { target: { value: "id" } });
    fireEvent.keyDown(pkInput, { key: "Enter" });
    const lastCall = onChange.mock.calls.at(-1)![0] as ObjectMappingDef;
    expect(lastCall.primary_key_columns).toEqual([
      { column: "id", relation: "public.customers" },
    ]);
  });

  it("removes a primary key column when its remove button is clicked", () => {
    const onChange = vi.fn();
    const seeded: ObjectMappingDef = {
      ...withRelation("public.customers"),
      primary_key_columns: [
        { column: "tenant_id", relation: "public.customers" },
        { column: "id", relation: "public.customers" },
      ],
    };
    renderEditor(
      <InlineObjectMappingEditor
        value={seeded}
        properties={PROPERTIES}
        onChange={onChange}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Remove id from primary key" }),
    );
    const lastCall = onChange.mock.calls.at(-1)![0] as ObjectMappingDef;
    expect(lastCall.primary_key_columns).toEqual([
      { column: "tenant_id", relation: "public.customers" },
    ]);
  });

  it("creates a property mapping when its column input is filled", () => {
    const onChange = vi.fn();
    renderEditor(
      <InlineObjectMappingEditor
        value={withRelation("public.customers")}
        properties={PROPERTIES}
        onChange={onChange}
      />,
    );
    const inputs = screen.getAllByPlaceholderText(
      "Column name (empty = unmapped)",
    );
    // Two property rows → two column inputs.
    expect(inputs).toHaveLength(2);
    fireEvent.change(inputs[0], { target: { value: "customer_id" } });
    const lastCall = onChange.mock.calls.at(-1)![0] as ObjectMappingDef;
    expect(lastCall.property_mappings).toHaveLength(1);
    expect(lastCall.property_mappings![0]).toMatchObject({
      property_id: "p-id",
      property_key: "id",
      location: {
        kind: "column",
        column: "customer_id",
        relation: "public.customers",
      },
      transform: { kind: "identity" },
    });
  });

  it("removes a property mapping when its column is cleared", () => {
    const onChange = vi.fn();
    const seeded: ObjectMappingDef = {
      ...withRelation("public.customers"),
      property_mappings: [
        {
          property_id: "p-id",
          property_key: "id",
          location: {
            kind: "column",
            column: "customer_id",
            relation: "public.customers",
          },
          transform: { kind: "identity" },
        },
      ],
    };
    renderEditor(
      <InlineObjectMappingEditor
        value={seeded}
        properties={PROPERTIES}
        onChange={onChange}
      />,
    );
    const inputs = screen.getAllByPlaceholderText(
      "Column name (empty = unmapped)",
    );
    fireEvent.change(inputs[0], { target: { value: "" } });
    const lastCall = onChange.mock.calls.at(-1)![0] as ObjectMappingDef;
    expect(lastCall.property_mappings).toEqual([]);
  });

  it("disables every input when readOnly", () => {
    renderEditor(
      <InlineObjectMappingEditor
        value={withRelation("public.customers")}
        properties={PROPERTIES}
        onChange={() => {}}
        readOnly
      />,
    );
    expect(
      screen.getByPlaceholderText("e.g. public.customers"),
    ).toBeDisabled();
    // No "+ column" input visible in read-only mode.
    expect(screen.queryByPlaceholderText("+ column")).toBeNull();
  });
});

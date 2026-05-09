import { describe, expect, it } from "vitest";

import { validateRecord } from "@/lib/forms/field-schema";
import type { ValueSetDef } from "@/lib/api/edit-ops";
import { valueSetSchema } from "./value-set.schema";

describe("valueSetSchema", () => {
  it("accepts every canonical value-set selector variant", () => {
    const base = valueSetSchema.buildDefault();

    const selectors: NonNullable<ValueSetDef["composition"]>[number]["selector"][] = [
      { kind: "all" },
      { kind: "explicit", codes: ["ACTIVE"] },
      { kind: "descendants_of", root_id: "cv-open" },
      { kind: "code_pattern", pattern: "^A-" },
    ];

    for (const selector of selectors) {
      expect(
        validateRecord(valueSetSchema, {
          ...base,
          id: "vs-order-status",
          name: "OrderStatus",
          composition: [{ system_id: "cs-order-status", mode: "include", selector }],
        }),
      ).toEqual([]);
    }
  });

  it("rejects empty parameterized selectors", () => {
    const base = {
      ...valueSetSchema.buildDefault(),
      id: "vs-order-status",
      name: "OrderStatus",
    };

    expect(
      validateRecord(valueSetSchema, {
        ...base,
        composition: [
          { system_id: "cs-order-status", mode: "include", selector: { kind: "explicit", codes: [] } },
        ],
      }),
    ).toContainEqual({ messageKey: "required", params: { field: "codes" } });

    expect(
      validateRecord(valueSetSchema, {
        ...base,
        composition: [
          { system_id: "cs-order-status", mode: "include", selector: { kind: "descendants_of", root_id: "" } },
        ],
      }),
    ).toContainEqual({ messageKey: "required", params: { field: "root_id" } });

    expect(
      validateRecord(valueSetSchema, {
        ...base,
        composition: [
          { system_id: "cs-order-status", mode: "include", selector: { kind: "code_pattern", pattern: "" } },
        ],
      }),
    ).toContainEqual({ messageKey: "required", params: { field: "pattern" } });
  });
});

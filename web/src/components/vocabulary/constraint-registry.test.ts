import { describe, expect, it } from "vitest";

import { CONSTRAINT_KINDS, constraintSpec } from "./constraint-registry";
import type { ShaclConstraint } from "@/lib/api/edit-ops";

describe("constraint registry", () => {
  it("serializes datatype constraints with tagged PropertyType", () => {
    const spec = constraintSpec("datatype");

    expect(spec?.toConstraint({ target: { kind: "inherit" }, expected: "date_time" })).toEqual({
      kind: "datatype",
      target: { kind: "inherit" },
      expected: { type: "date_time" },
    });
  });

  it("serializes sibling property comparison constraints", () => {
    expect(
      constraintSpec("less_than")?.toConstraint({
        target: { kind: "inherit" },
        other_property: "closed_at",
      }),
    ).toEqual({
      kind: "less_than",
      target: { kind: "inherit" },
      other_property: "closed_at",
    });
    expect(
      constraintSpec("equals")?.toConstraint({
        target: { kind: "inherit" },
        other_property: "normalized_status",
      }),
    ).toEqual({
      kind: "equals",
      target: { kind: "inherit" },
      other_property: "normalized_status",
    });
  });

  it("registers every generated SHACL constraint variant", () => {
    const expected = [
      "min_count",
      "max_count",
      "datatype",
      "matches_pattern",
      "in_value_set",
      "has_value",
      "min_inclusive",
      "max_inclusive",
      "min_length",
      "max_length",
      "unique_lang",
      "closed",
      "disjoint",
      "unique_key",
      "less_than",
      "equals",
      "or",
      "and",
      "not",
      "xone",
      "qualified_value_shape",
    ] satisfies ShaclConstraint["kind"][];

    expect(CONSTRAINT_KINDS).toEqual(expected);
  });

  it("serializes recursive SHACL constraints without flattening branches", () => {
    const branch: ShaclConstraint = {
      kind: "datatype",
      target: { kind: "inherit" },
      expected: { type: "string" },
    };

    expect(
      constraintSpec("or")?.toConstraint({
        branches: [branch, { kind: "min_count", target: { kind: "inherit" }, min: 1 }],
      }),
    ).toEqual({
      kind: "or",
      branches: [branch, { kind: "min_count", target: { kind: "inherit" }, min: 1 }],
    });
    expect(
      constraintSpec("not")?.toConstraint({
        inner: branch,
      }),
    ).toEqual({
      kind: "not",
      inner: branch,
    });
    expect(
      constraintSpec("qualified_value_shape")?.toConstraint({
        shape: branch,
        qualified_min_count: 1,
        qualified_max_count: undefined,
      }),
    ).toEqual({
      kind: "qualified_value_shape",
      shape: branch,
      qualified_min_count: 1,
    });
  });
});

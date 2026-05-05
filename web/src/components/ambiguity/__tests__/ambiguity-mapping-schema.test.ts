import { describe, it, expect } from "vitest";

import {
  AmbiguityMappingFormSchema,
  toAmbiguityMapping,
} from "../ambiguity-mapping-schema";

describe("AmbiguityMappingFormSchema — value_map", () => {
  it("accepts at least one row with both value and display non-empty", () => {
    const ok = AmbiguityMappingFormSchema.parse({
      kind: "value_map",
      entries: [{ value: "A", display: "Alpha", definition: "" }],
    });
    expect(toAmbiguityMapping(ok)).toEqual({
      kind: "value_map",
      entries: [{ value: "A", display: "Alpha", definition: undefined }],
    });
  });

  it("rejects when no row has both value and display filled", () => {
    const result = AmbiguityMappingFormSchema.safeParse({
      kind: "value_map",
      entries: [{ value: "A", display: "", definition: "" }],
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0].message).toBe("errors.valueMapEmpty");
    }
  });

  it("filters incomplete rows out of the wire payload", () => {
    const ok = AmbiguityMappingFormSchema.parse({
      kind: "value_map",
      entries: [
        { value: "A", display: "Alpha", definition: "first letter" },
        { value: "B", display: "", definition: "incomplete" },
        { value: "", display: "Charlie", definition: "missing value" },
      ],
    });
    expect(toAmbiguityMapping(ok)).toEqual({
      kind: "value_map",
      entries: [
        { value: "A", display: "Alpha", definition: "first letter" },
      ],
    });
  });

  it("elides blank definitions to undefined", () => {
    const ok = AmbiguityMappingFormSchema.parse({
      kind: "value_map",
      entries: [{ value: "A", display: "Alpha", definition: "   " }],
    });
    expect(toAmbiguityMapping(ok).kind).toBe("value_map");
    if (toAmbiguityMapping(ok).kind === "value_map") {
      const mapping = toAmbiguityMapping(ok);
      if (mapping.kind !== "value_map") return;
      expect(mapping.entries[0].definition).toBeUndefined();
    }
  });
});

describe("AmbiguityMappingFormSchema — code_system_ref", () => {
  it("requires non-empty code_system_id", () => {
    const result = AmbiguityMappingFormSchema.safeParse({
      kind: "code_system_ref",
      code_system_id: "   ",
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0].message).toBe(
        "errors.codeSystemRequired",
      );
    }
  });

  it("trims and round-trips to wire shape", () => {
    const ok = AmbiguityMappingFormSchema.parse({
      kind: "code_system_ref",
      code_system_id: "  cs-orders  ",
    });
    expect(toAmbiguityMapping(ok)).toEqual({
      kind: "code_system_ref",
      code_system_id: "cs-orders",
    });
  });
});

describe("AmbiguityMappingFormSchema — glossary_ref", () => {
  it("requires non-empty term_id", () => {
    const result = AmbiguityMappingFormSchema.safeParse({
      kind: "glossary_ref",
      term_id: "",
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0].message).toBe("errors.termRequired");
    }
  });

  it("trims and round-trips to wire shape", () => {
    const ok = AmbiguityMappingFormSchema.parse({
      kind: "glossary_ref",
      term_id: "  g-vip  ",
    });
    expect(toAmbiguityMapping(ok)).toEqual({
      kind: "glossary_ref",
      term_id: "g-vip",
    });
  });
});

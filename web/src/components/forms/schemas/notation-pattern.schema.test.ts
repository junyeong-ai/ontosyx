import { describe, expect, it } from "vitest";

import { validateRecord } from "@/lib/forms/field-schema";
import type { NotationPatternDef } from "@/lib/api/edit-ops";
import { notationPatternSchema } from "./notation-pattern.schema";

describe("notationPatternSchema", () => {
  it("accepts every canonical notation component kind", () => {
    const base = {
      ...notationPatternSchema.buildDefault(),
      id: "np-campaign-code",
      name: "CampaignCode",
      template: "{campaign}_{year}_{seq}",
    };
    const components: NotationPatternDef["components"] = [
      {
        name: "campaign",
        kind: { kind: "code_from_set", value_set_id: "vs-campaign" },
      },
      {
        name: "year",
        kind: { kind: "integer_range", min: 0, max: 99, width: 2 },
      },
      {
        name: "seq",
        kind: { kind: "alphanumeric", width: 4, uppercase: true },
      },
      {
        name: "note",
        kind: { kind: "free_text", max_len: 24 },
      },
    ];

    expect(
      validateRecord(notationPatternSchema, {
        ...base,
        components,
      }),
    ).toEqual([]);
  });

  it("rejects empty component lists and invalid integer ranges", () => {
    const base = {
      ...notationPatternSchema.buildDefault(),
      id: "np-campaign-code",
      name: "CampaignCode",
      template: "{year}",
    };

    expect(validateRecord(notationPatternSchema, base)).toContainEqual({
      messageKey: "required",
      params: { field: "components" },
    });

    expect(
      validateRecord(notationPatternSchema, {
        ...base,
        components: [
          {
            name: "year",
            kind: { kind: "integer_range", min: 99, max: 0, width: 2 },
          },
        ],
      }),
    ).toContainEqual({ messageKey: "rangeOrder", params: { field: "max" } });
  });
});

import { describe, it, expect } from "vitest";

import { fromPatternIR, type WirePatternIR } from "./saved-pattern-io";

describe("fromPatternIR — read_only_reason passthrough", () => {
  it("returns undefined readOnlyReason on a pattern without the field", () => {
    // Common case: a Match-decompile PatternIR carries no
    // `read_only_reason` on the wire. The result's `readOnlyReason`
    // must be `undefined` so the UI renders the canvas as editable.
    const wire: WirePatternIR = {
      schema_version: 1,
      nodes: [
        {
          id: "n1",
          variable: "p",
          label: "Person",
        },
      ],
    };
    const { readOnlyReason } = fromPatternIR(wire);
    expect(readOnlyReason).toBeUndefined();
  });

  it("passes through readOnlyReason verbatim for a non-Match decompile", () => {
    // The backend emits an empty-nodes PatternIR plus a
    // `read_only_reason` naming the Rust `QueryOp` variant. The UI
    // relies on that pass-through for its "locked: <op>" banner —
    // any rename of this wire field would surface here first.
    const wire: WirePatternIR = {
      schema_version: 1,
      nodes: [],
      read_only_reason: { original_op: "Aggregate" },
    };
    const result = fromPatternIR(wire);
    expect(result.readOnlyReason).toEqual({ original_op: "Aggregate" });
    expect(result.visual.nodes).toHaveLength(0);
  });

  it("pairs an empty canvas with a concrete op name (not just emptiness)", () => {
    // Regression anchor for the gap the backend change closed: before
    // `read_only_reason`, an empty PatternIR was ambiguous between
    // "blank new query" and "unsupported operation collapsed to
    // empty". The wire-level op name now disambiguates.
    const unknown: WirePatternIR = { schema_version: 1, nodes: [] };
    const locked: WirePatternIR = {
      schema_version: 1,
      nodes: [],
      read_only_reason: { original_op: "PathFind" },
    };
    expect(fromPatternIR(unknown).readOnlyReason).toBeUndefined();
    expect(fromPatternIR(locked).readOnlyReason?.original_op).toBe("PathFind");
  });
});

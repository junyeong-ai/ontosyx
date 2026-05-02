// Reference-equality stability for selectors. UI components
// subscribe through these — returning a fresh `new Map()` /
// `new Array()` on every render would break Zustand's
// shallow-equality detection and trigger spurious re-renders.

import { describe, it, expect } from "vitest";

import {
  selectCursors,
  selectLocks,
  selectPresence,
  type CollabState,
} from "../store";

const empty: CollabState = {
  connectionState: "idle",
  lastError: null,
  rooms: new Map(),
  setConnectionState: () => {},
  applyServerMessage: () => {},
  reset: () => {},
};

const projectId = "00000000-0000-0000-0000-0000000000aa";

describe("collab selectors return stable empties", () => {
  it("selectPresence returns the same array reference for an unseeded room", () => {
    const a = selectPresence(projectId)(empty);
    const b = selectPresence(projectId)(empty);
    expect(a).toBe(b);
  });

  it("selectCursors returns the same map reference for an unseeded room", () => {
    const a = selectCursors(projectId)(empty);
    const b = selectCursors(projectId)(empty);
    expect(a).toBe(b);
  });

  it("selectLocks returns the same map reference for an unseeded room", () => {
    const a = selectLocks(projectId)(empty);
    const b = selectLocks(projectId)(empty);
    expect(a).toBe(b);
  });
});

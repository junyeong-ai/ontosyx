// Reference-equality stability for selectors. UI components
// subscribe through these — returning a fresh `new Map()` /
// `new Array()` on every render would break Zustand's
// shallow-equality detection and trigger spurious re-renders.

import { describe, it, expect } from "vitest";

import {
  selectStateCursors,
  selectStateLocks,
  selectStatePresence,
  type CollabState,
} from "../store";

const empty: CollabState = {
  connectionState: "idle",
  clientReady: false,
  lastError: null,
  rooms: new Map(),
  setConnectionState: () => {},
  setClientReady: () => {},
  applyServerMessage: () => {},
  ackRemoteUpdate: () => {},
  reset: () => {},
  hidden: false,
  setHidden: () => {},
};

const ontologyDraftId = "00000000-0000-0000-0000-0000000000aa";

describe("collab selectors return stable empties", () => {
  it("selectStatePresence returns the same array reference for an unseeded room", () => {
    const a = selectStatePresence(ontologyDraftId)(empty);
    const b = selectStatePresence(ontologyDraftId)(empty);
    expect(a).toBe(b);
  });

  it("selectStateCursors returns the same map reference for an unseeded room", () => {
    const a = selectStateCursors(ontologyDraftId)(empty);
    const b = selectStateCursors(ontologyDraftId)(empty);
    expect(a).toBe(b);
  });

  it("selectStateLocks returns the same map reference for an unseeded room", () => {
    const a = selectStateLocks(ontologyDraftId)(empty);
    const b = selectStateLocks(ontologyDraftId)(empty);
    expect(a).toBe(b);
  });
});

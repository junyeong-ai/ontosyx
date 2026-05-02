import { describe, it, expect } from "vitest";

import { applyServerMessage, type CollabState } from "../store";
import type { ServerMessage } from "../types";

const initial: CollabState = {
  connectionState: "idle",
  clientReady: false,
  lastError: null,
  rooms: new Map(),
  setConnectionState: () => {},
  setClientReady: () => {},
  applyServerMessage: () => {},
  reset: () => {},
  hidden: false,
  setHidden: () => {},
};

const projectA = "00000000-0000-0000-0000-0000000000aa";

describe("applyServerMessage", () => {
  it("seeds the room from a Presence snapshot", () => {
    const msg: ServerMessage = {
      type: "presence",
      project_id: projectA,
      users: [
        {
          user_id: "u1",
          user_name: "Alice",
          joined_at: "2026-05-02T00:00:00Z",
          cursor: null,
        },
      ],
      locks: [],
    };
    const next = applyServerMessage(initial, msg);
    expect(next.rooms?.get(projectA)?.presence).toHaveLength(1);
  });

  it("seeds active locks from a Presence snapshot", () => {
    const msg: ServerMessage = {
      type: "presence",
      project_id: projectA,
      users: [],
      locks: [
        {
          entity_id: "ent-1",
          held_by: "u1",
          expires_at: "2026-05-02T00:05:00Z",
        },
      ],
    };
    const next = applyServerMessage(initial, msg);
    const lock = next.rooms?.get(projectA)?.locks.get("ent-1");
    expect(lock?.heldBy).toBe("u1");
    expect(lock?.expiresAt).toBe("2026-05-02T00:05:00Z");
  });

  it("dedupes UserJoined when the joiner is already in presence", () => {
    const seeded: CollabState = {
      ...initial,
      rooms: new Map([
        [
          projectA,
          {
            presence: [
              {
                user_id: "u1",
                user_name: "Alice",
                joined_at: "2026-05-02T00:00:00Z",
                cursor: null,
              },
            ],
            cursors: new Map(),
            locks: new Map(),
          },
        ],
      ]),
    };
    const msg: ServerMessage = {
      type: "user_joined",
      project_id: projectA,
      user: {
        user_id: "u1",
        user_name: "Alice",
        joined_at: "2026-05-02T00:00:01Z",
        cursor: null,
      },
    };
    const next = applyServerMessage(seeded, msg);
    expect(next.rooms?.get(projectA)?.presence).toHaveLength(1);
  });

  it("removes presence + cursor on UserLeft", () => {
    const seeded: CollabState = {
      ...initial,
      rooms: new Map([
        [
          projectA,
          {
            presence: [
              {
                user_id: "u1",
                user_name: "Alice",
                joined_at: "2026-05-02T00:00:00Z",
                cursor: null,
              },
              {
                user_id: "u2",
                user_name: "Bob",
                joined_at: "2026-05-02T00:00:00Z",
                cursor: null,
              },
            ],
            cursors: new Map([
              ["u1", { x: 0, y: 0, selected_element: null, lastUpdateAt: 0 }],
            ]),
            locks: new Map(),
          },
        ],
      ]),
    };
    const msg: ServerMessage = {
      type: "user_left",
      project_id: projectA,
      user_id: "u1",
    };
    const next = applyServerMessage(seeded, msg);
    const room = next.rooms?.get(projectA);
    expect(room?.presence.map((p) => p.user_id)).toEqual(["u2"]);
    expect(room?.cursors.has("u1")).toBe(false);
  });

  it("stores LockGranted with held_by + expires_at", () => {
    const msg: ServerMessage = {
      type: "lock_granted",
      project_id: projectA,
      entity_id: "ent-1",
      held_by: "u1",
      expires_at: "2026-05-02T00:05:00Z",
    };
    const next = applyServerMessage(initial, msg);
    const lock = next.rooms?.get(projectA)?.locks.get("ent-1");
    expect(lock?.heldBy).toBe("u1");
    expect(lock?.expiresAt).toBe("2026-05-02T00:05:00Z");
  });

  it("clears LockReleased entries", () => {
    const seeded: CollabState = {
      ...initial,
      rooms: new Map([
        [
          projectA,
          {
            presence: [],
            cursors: new Map(),
            locks: new Map([
              ["ent-1", { heldBy: "u1", expiresAt: "2026-05-02T00:05:00Z" }],
            ]),
          },
        ],
      ]),
    };
    const msg: ServerMessage = {
      type: "lock_released",
      project_id: projectA,
      entity_id: "ent-1",
    };
    const next = applyServerMessage(seeded, msg);
    expect(next.rooms?.get(projectA)?.locks.has("ent-1")).toBe(false);
  });

  it("records cursor position with selected_element + idle timestamp", () => {
    const msg: ServerMessage = {
      type: "remote_cursor",
      project_id: projectA,
      user_id: "u1",
      user_name: "Alice",
      x: 10,
      y: 20,
      selected_element: "node-42",
    };
    const before = Date.now();
    const next = applyServerMessage(initial, msg);
    const cursor = next.rooms?.get(projectA)?.cursors.get("u1");
    expect(cursor?.x).toBe(10);
    expect(cursor?.y).toBe(20);
    expect(cursor?.selected_element).toBe("node-42");
    expect(cursor?.lastUpdateAt).toBeGreaterThanOrEqual(before);
  });

  it("captures Error frames into lastError", () => {
    const msg: ServerMessage = {
      type: "error",
      code: "session_revoked",
      params: {},
    };
    const next = applyServerMessage(initial, msg);
    expect(next.lastError?.code).toBe("session_revoked");
  });
});

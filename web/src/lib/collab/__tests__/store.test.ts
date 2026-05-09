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
  ackRemoteUpdate: () => {},
  reset: () => {},
  hidden: false,
  setHidden: () => {},
};

const projectA = "00000000-0000-0000-0000-0000000000aa";

describe("applyServerMessage", () => {
  it("seeds the room from a Presence snapshot", () => {
    const msg: ServerMessage = {
      type: "presence",
      ontology_draft_id: projectA,
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
      ontology_draft_id: projectA,
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
      ontology_draft_id: projectA,
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
      ontology_draft_id: projectA,
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
      ontology_draft_id: projectA,
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
      ontology_draft_id: projectA,
      entity_id: "ent-1",
    };
    const next = applyServerMessage(seeded, msg);
    expect(next.rooms?.get(projectA)?.locks.has("ent-1")).toBe(false);
  });

  it("records cursor position with selected_element + idle timestamp", () => {
    const msg: ServerMessage = {
      type: "remote_cursor",
      ontology_draft_id: projectA,
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

  it("captures EntityUpdated as the room's latestRemoteUpdate snapshot", () => {
    const msg: ServerMessage = {
      type: "entity_updated",
      ontology_draft_id: projectA,
      author_user_id: "u-bob",
      author_user_name: "Bob",
      base_revision: 5,
      new_revision: 6,
      commands: [
        { op: "rename_node", node_id: "n1", new_label: "Customer" },
      ],
    };
    const next = applyServerMessage(initial, msg);
    const remote = next.rooms?.get(projectA)?.latestRemoteUpdate;
    expect(remote?.authorUserId).toBe("u-bob");
    expect(remote?.authorUserName).toBe("Bob");
    expect(remote?.baseRevision).toBe(5);
    expect(remote?.newRevision).toBe(6);
    expect(remote?.commands).toHaveLength(1);
  });
});

// Zustand slice that distills server messages into per-project room
// state. UI components subscribe via the selectors at the bottom of
// the file rather than reading raw server frames.

import { create } from "zustand";

import type {
  ConnectionState,
} from "./client";
import type {
  CursorPosition,
  LockState,
  PresenceInfo,
  ServerMessage,
} from "./types";

/** Snapshot of one collaboration room. */
export interface RoomState {
  presence: PresenceInfo[];
  /** user_id → live cursor */
  cursors: Map<string, CursorPosition>;
  /** entity_id → lock holder + expiry */
  locks: Map<string, LockState>;
}

const emptyRoom = (): RoomState => ({
  presence: [],
  cursors: new Map(),
  locks: new Map(),
});

export interface CollabState {
  /** Connection lifecycle — drives banner / retry UI. */
  connectionState: ConnectionState;
  /** Last `Error` frame, kept until cleared or replaced. */
  lastError: { code: string; params: Record<string, string> } | null;
  /** project_id → room state */
  rooms: Map<string, RoomState>;

  setConnectionState(state: ConnectionState): void;
  applyServerMessage(msg: ServerMessage): void;
  reset(): void;
}

export const useCollabStore = create<CollabState>((set) => ({
  connectionState: "idle",
  lastError: null,
  rooms: new Map(),

  setConnectionState(state) {
    set({ connectionState: state });
  },

  applyServerMessage(msg) {
    set((s) => applyServerMessage(s, msg));
  },

  reset() {
    set({ connectionState: "idle", lastError: null, rooms: new Map() });
  },
}));

/**
 * Pure reducer — exported for tests. The Zustand action wraps it
 * so we don't duplicate the dispatch table inside `set(...)`.
 */
export function applyServerMessage(
  state: CollabState,
  msg: ServerMessage,
): Partial<CollabState> {
  switch (msg.type) {
    case "authenticated":
      // Connection lifecycle is tracked separately; auth ack itself
      // doesn't alter rooms.
      return {};

    case "presence": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.project_id) ?? emptyRoom();
      // Snapshot replaces the presence list verbatim; cursors and
      // locks survive — they may have been seeded by frames that
      // arrived before this snapshot.
      rooms.set(msg.project_id, {
        ...room,
        presence: msg.users,
      });
      return { rooms };
    }

    case "user_joined": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.project_id) ?? emptyRoom();
      // `user_joined` is broadcast — the joining socket also sees
      // its own join, so dedupe by user_id rather than appending.
      const presence = room.presence.filter(
        (p) => p.user_id !== msg.user.user_id,
      );
      presence.push(msg.user);
      rooms.set(msg.project_id, { ...room, presence });
      return { rooms };
    }

    case "user_left": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.project_id);
      if (!room) return {};
      const presence = room.presence.filter((p) => p.user_id !== msg.user_id);
      const cursors = new Map(room.cursors);
      cursors.delete(msg.user_id);
      // Locks the leaver held are released by separate
      // `lock_released` frames — don't reap here, lest we drop a
      // lock the server hasn't yet broadcast the release for.
      rooms.set(msg.project_id, { ...room, presence, cursors });
      return { rooms };
    }

    case "remote_cursor": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.project_id) ?? emptyRoom();
      const cursors = new Map(room.cursors);
      cursors.set(msg.user_id, {
        x: msg.x,
        y: msg.y,
        selected_element: msg.selected_element,
      });
      rooms.set(msg.project_id, { ...room, cursors });
      return { rooms };
    }

    case "lock_granted": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.project_id) ?? emptyRoom();
      const locks = new Map(room.locks);
      locks.set(msg.entity_id, {
        heldBy: msg.held_by,
        expiresAt: msg.expires_at,
      });
      rooms.set(msg.project_id, { ...room, locks });
      return { rooms };
    }

    case "lock_released": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.project_id);
      if (!room) return {};
      const locks = new Map(room.locks);
      locks.delete(msg.entity_id);
      rooms.set(msg.project_id, { ...room, locks });
      return { rooms };
    }

    case "lock_denied":
      // Denials don't mutate state — the requester's UI shows a
      // toast / inline marker keyed off the held_by identity.
      // Surfacing through `lastError` would conflate user-facing
      // errors (auth, etc.) with workflow signals.
      return {};

    case "error":
      return { lastError: { code: msg.code, params: msg.params } };

    default:
      return {};
  }
}

// ---------------------------------------------------------------------------
// Selectors — UI consumers read state through these so component
// re-renders are scoped to the data they use.
// ---------------------------------------------------------------------------

export const selectPresence = (projectId: string) =>
  (state: CollabState): PresenceInfo[] =>
    state.rooms.get(projectId)?.presence ?? [];

export const selectCursors = (projectId: string) =>
  (state: CollabState): Map<string, CursorPosition> =>
    state.rooms.get(projectId)?.cursors ?? new Map();

export const selectLocks = (projectId: string) =>
  (state: CollabState): Map<string, LockState> =>
    state.rooms.get(projectId)?.locks ?? new Map();

export const selectLockFor = (projectId: string, entityId: string) =>
  (state: CollabState): LockState | undefined =>
    state.rooms.get(projectId)?.locks.get(entityId);

export const selectConnectionState = (state: CollabState): ConnectionState =>
  state.connectionState;

export const selectLastError = (state: CollabState) => state.lastError;

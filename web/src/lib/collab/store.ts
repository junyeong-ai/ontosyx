// Zustand slice that distills server messages into per-project room
// state. UI components subscribe via the selectors at the bottom of
// the file rather than reading raw server frames.

import { create } from "zustand";

import type {
  ConnectionState,
} from "./client";
import type { OntologyCommand } from "@/types/api";
import type {
  CursorPosition,
  LockState,
  PresenceInfo,
  ServerMessage,
} from "./types";

// Module-level singletons returned from selectors when a room
// hasn't been seeded yet. Returning a fresh `new Map()` /
// `new Array()` every render would break Zustand's reference-
// equality tracking and trigger pointless re-renders for any
// component that selects these slices.
const EMPTY_PRESENCE: readonly PresenceInfo[] = Object.freeze([]);
const EMPTY_CURSORS: ReadonlyMap<string, CursorEntry> = new Map();
const EMPTY_LOCKS: ReadonlyMap<string, LockState> = new Map();

/**
 * Cursor position augmented with a wall-clock timestamp the
 * renderer uses to fade idle cursors. The wire shape
 * (`CursorPosition`) stays unchanged; the timestamp is internal,
 * stamped by the reducer when the frame lands.
 */
export interface CursorEntry extends CursorPosition {
  /** `Date.now()` at the moment the most recent frame arrived. */
  lastUpdateAt: number;
}

/**
 * The latest `entity_updated` frame observed for a project. The
 * inspector panel reads this on a 409 to populate
 * `conflict.remoteCommands` so the merge dialog renders the
 * symmetric remote-ops inventory instead of the opaque fallback.
 *
 * Stored per-room rather than queued because conflict resolution is
 * "all-or-nothing" — Keep mine rebases atop the freshly fetched
 * server state, Take theirs accepts it verbatim. The user always
 * resolves against the latest known remote commit.
 */
export interface RemoteUpdateSnapshot {
  authorUserId: string;
  authorUserName: string;
  baseRevision: number;
  newRevision: number;
  commands: readonly OntologyCommand[];
}

/** Snapshot of one collaboration room. */
export interface RoomState {
  presence: PresenceInfo[];
  /** user_id → live cursor */
  cursors: Map<string, CursorEntry>;
  /** entity_id → lock holder + expiry */
  locks: Map<string, LockState>;
  /** Latest `entity_updated` for the room, or `undefined` until one
   *  arrives. Cleared after the conflict surface resolves. */
  latestRemoteUpdate?: RemoteUpdateSnapshot;
}

const emptyRoom = (): RoomState => ({
  presence: [],
  cursors: new Map(),
  locks: new Map(),
});

export interface CollabState {
  /** Connection lifecycle — drives banner / retry UI. */
  connectionState: ConnectionState;
  /**
   * `true` once the singleton `CollaborationClient` has been
   * constructed (post `useCollab` mount). Room-level hooks gate
   * `join` on this so they don't fire against a null client and
   * silently miss the rejoin set.
   */
  clientReady: boolean;
  /** Last `Error` frame, kept until cleared or replaced. */
  lastError: { code: string; params: Record<string, string> } | null;
  /** ontology_draft_id → room state */
  rooms: Map<string, RoomState>;
  /**
   * `true` when the tab is hidden. Cursor-emitting components
   * read this to short-circuit publishes — presence stays
   * accurate but bandwidth drops to zero when the user can't
   * see the canvas anyway.
   */
  hidden: boolean;

  setConnectionState(state: ConnectionState): void;
  setClientReady(ready: boolean): void;
  setHidden(hidden: boolean): void;
  applyServerMessage(msg: ServerMessage): void;
  /** Clear `latestRemoteUpdate` for a project — called after the
   *  conflict resolution flow consumes it (Keep mine / Take theirs)
   *  so the next 409 starts from a clean slate. */
  ackRemoteUpdate(ontologyDraftId: string): void;
  reset(): void;
}

export const useCollabStore = create<CollabState>((set) => ({
  connectionState: "idle",
  clientReady: false,
  lastError: null,
  rooms: new Map(),
  hidden: false,

  setConnectionState(state) {
    set({ connectionState: state });
  },

  setClientReady(ready) {
    set({ clientReady: ready });
  },

  setHidden(hidden) {
    set({ hidden });
  },

  applyServerMessage(msg) {
    set((s) => applyServerMessage(s, msg));
  },

  ackRemoteUpdate(ontologyDraftId) {
    set((s) => {
      const room = s.rooms.get(ontologyDraftId);
      if (!room?.latestRemoteUpdate) return {};
      const rooms = new Map(s.rooms);
      rooms.set(ontologyDraftId, {
        presence: room.presence,
        cursors: room.cursors,
        locks: room.locks,
      });
      return { rooms };
    });
  },

  reset() {
    set({
      connectionState: "idle",
      clientReady: false,
      lastError: null,
      rooms: new Map(),
      hidden: false,
    });
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
      const room = rooms.get(msg.ontology_draft_id) ?? emptyRoom();
      // Snapshot is atomic — presence + locks come from the
      // server's authoritative view of the room. Cursors are
      // ephemeral (no server-side store) and survive so a
      // reconnect doesn't blank live cursors before the next
      // `RemoteCursor` frame arrives.
      const locks = new Map<string, LockState>();
      for (const lock of msg.locks) {
        locks.set(lock.entity_id, {
          heldBy: lock.held_by,
          expiresAt: lock.expires_at,
        });
      }
      rooms.set(msg.ontology_draft_id, {
        ...room,
        presence: msg.users,
        locks,
      });
      return { rooms };
    }

    case "user_joined": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.ontology_draft_id) ?? emptyRoom();
      // `user_joined` is broadcast — the joining socket also sees
      // its own join, so dedupe by user_id rather than appending.
      const presence = room.presence.filter(
        (p) => p.user_id !== msg.user.user_id,
      );
      presence.push(msg.user);
      rooms.set(msg.ontology_draft_id, { ...room, presence });
      return { rooms };
    }

    case "user_left": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.ontology_draft_id);
      if (!room) return {};
      const presence = room.presence.filter((p) => p.user_id !== msg.user_id);
      const cursors = new Map(room.cursors);
      cursors.delete(msg.user_id);
      // Locks the leaver held are released by separate
      // `lock_released` frames — don't reap here, lest we drop a
      // lock the server hasn't yet broadcast the release for.
      rooms.set(msg.ontology_draft_id, { ...room, presence, cursors });
      return { rooms };
    }

    case "remote_cursor": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.ontology_draft_id) ?? emptyRoom();
      const cursors = new Map(room.cursors);
      cursors.set(msg.user_id, {
        x: msg.x,
        y: msg.y,
        selected_element: msg.selected_element,
        lastUpdateAt: Date.now(),
      });
      rooms.set(msg.ontology_draft_id, { ...room, cursors });
      return { rooms };
    }

    case "lock_granted": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.ontology_draft_id) ?? emptyRoom();
      const locks = new Map(room.locks);
      locks.set(msg.entity_id, {
        heldBy: msg.held_by,
        expiresAt: msg.expires_at,
      });
      rooms.set(msg.ontology_draft_id, { ...room, locks });
      return { rooms };
    }

    case "lock_released": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.ontology_draft_id);
      if (!room) return {};
      const locks = new Map(room.locks);
      locks.delete(msg.entity_id);
      rooms.set(msg.ontology_draft_id, { ...room, locks });
      return { rooms };
    }

    case "lock_denied":
      // Denials don't mutate state — the requester's UI shows a
      // toast / inline marker keyed off the held_by identity.
      // Surfacing through `lastError` would conflate user-facing
      // errors (auth, etc.) with workflow signals.
      return {};

    case "entity_updated": {
      const rooms = new Map(state.rooms);
      const room = rooms.get(msg.ontology_draft_id) ?? emptyRoom();
      // The OpenAPI surface types `commands` as
      // `Record<string, never>[]` because `OntologyCommand` is an
      // internally-tagged union utoipa can't derive a static schema
      // for. The runtime payload is the canonical wire shape — narrow
      // it through `unknown` at the single wire boundary so downstream
      // consumers see the typed union.
      const commands = msg.commands as unknown as readonly OntologyCommand[];
      rooms.set(msg.ontology_draft_id, {
        ...room,
        latestRemoteUpdate: {
          authorUserId: msg.author_user_id,
          authorUserName: msg.author_user_name,
          baseRevision: msg.base_revision,
          newRevision: msg.new_revision,
          commands,
        },
      });
      return { rooms };
    }

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

export const selectStatePresence =
  (ontologyDraftId: string) =>
  (state: CollabState): readonly PresenceInfo[] =>
    state.rooms.get(ontologyDraftId)?.presence ?? EMPTY_PRESENCE;

export const selectStateCursors =
  (ontologyDraftId: string) =>
  (state: CollabState): ReadonlyMap<string, CursorEntry> =>
    state.rooms.get(ontologyDraftId)?.cursors ?? EMPTY_CURSORS;

export const selectStateLocks =
  (ontologyDraftId: string) =>
  (state: CollabState): ReadonlyMap<string, LockState> =>
    state.rooms.get(ontologyDraftId)?.locks ?? EMPTY_LOCKS;

export const selectStateLockFor =
  (ontologyDraftId: string, entityId: string) =>
  (state: CollabState): LockState | undefined =>
    state.rooms.get(ontologyDraftId)?.locks.get(entityId);

export const selectStateLatestRemoteUpdate =
  (ontologyDraftId: string) =>
  (state: CollabState): RemoteUpdateSnapshot | undefined =>
    state.rooms.get(ontologyDraftId)?.latestRemoteUpdate;

export const selectStateConnectionState = (
  state: CollabState,
): ConnectionState => state.connectionState;

export const selectStateLastError = (state: CollabState) => state.lastError;

export const selectStateHidden = (state: CollabState): boolean => state.hidden;

export const selectStateClientReady = (state: CollabState): boolean =>
  state.clientReady;

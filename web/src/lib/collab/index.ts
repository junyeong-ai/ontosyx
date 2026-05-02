// Public surface of the collaboration module. UI consumers import
// from here; internal modules can still reach in through the
// individual files above.

export { CollaborationClient } from "./client";
export type {
  CollaborationClientConfig,
  ConnectionState,
} from "./client";
export {
  applyServerMessage,
  selectConnectionState,
  selectCursors,
  selectLastError,
  selectLockFor,
  selectLocks,
  selectPresence,
  useCollabStore,
} from "./store";
export type { CollabState, RoomState } from "./store";
export { clearCollabClient, useCollab } from "./hooks";
export type { UseCollabOptions } from "./hooks";
export type {
  ClientMessage,
  ClientOf,
  CursorPosition,
  ErrorCode,
  LockState,
  PresenceInfo,
  ServerMessage,
  ServerOf,
} from "./types";

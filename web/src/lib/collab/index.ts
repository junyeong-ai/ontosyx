// Public surface of the collaboration module. UI consumers import
// from here; internal modules can still reach in through the
// individual files above.

export { clearWsTokenCache, fetchWsToken } from "./auth";
export { CollaborationClient } from "./client";
export type {
  CollaborationClientConfig,
  ConnectionState,
} from "./client";
export {
  applyServerMessage,
  selectClientReady,
  selectConnectionState,
  selectCursors,
  selectHidden,
  selectLastError,
  selectLockFor,
  selectLocks,
  selectPresence,
  useCollabStore,
} from "./store";
export { useNetworkAwareness, useVisibilityAwareness } from "./network-awareness";
export type { CollabState, RoomState } from "./store";
export { clearCollabClient, useCollab, useCollabRoom } from "./hooks";
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

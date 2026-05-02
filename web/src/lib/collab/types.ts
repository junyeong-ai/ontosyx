// Wire types for the collaboration WebSocket protocol. The server
// emits these schemas into `components/schemas` (see ox-api's
// `crate::collaboration`); we re-export here so consumers don't
// reach into the generated bundle directly.

import type { components } from "@/types/api.generated";

export type ClientMessage = components["schemas"]["ClientMessage"];
export type ServerMessage = components["schemas"]["ServerMessage"];
export type ErrorCode = components["schemas"]["ErrorCode"];
export type PresenceInfo = components["schemas"]["PresenceInfo"];
export type CursorPosition = components["schemas"]["CursorPosition"];
export type LockSnapshot = components["schemas"]["LockSnapshot"];

/**
 * Type-narrowed extractor for a `ServerMessage` variant. Lets call
 * sites switch on `msg.type` and pull a fully typed payload without
 * casting:
 *
 * ```ts
 * if (msg.type === "presence") {
 *   const payload: ServerOf<"presence"> = msg;
 * }
 * ```
 */
export type ServerOf<K extends ServerMessage["type"]> = Extract<
  ServerMessage,
  { type: K }
>;

export type ClientOf<K extends ClientMessage["type"]> = Extract<
  ClientMessage,
  { type: K }
>;

/**
 * Per-room lock state derived from `LockGranted` / `LockReleased`
 * frames. `expiresAt` lets the FE render countdown UIs and renew
 * before the TTL.
 */
export interface LockState {
  heldBy: string;
  expiresAt: string;
}

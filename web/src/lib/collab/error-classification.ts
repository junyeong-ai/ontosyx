// Single source of truth for how each `ServerMessage::Error` code
// surfaces to the user. Keeping the classification table in one
// module guarantees that the toaster, the session-expired overlay,
// and any future surface read the same policy — adding a new code
// requires editing this file once and the consequences fall out.

import type { ErrorCode } from "./types";

/** How an error code surfaces in the UI. */
export type ErrorSurface = "transient" | "recoverable" | "reauth";

/** Strongly-typed map. Adding a new code to the OpenAPI enum but
 *  forgetting to register it here is caught at compile time. */
const SURFACE_BY_CODE: Record<ErrorCode, ErrorSurface> = {
  auth_required: "reauth",
  auth_invalid: "reauth",
  auth_timeout: "reauth",
  session_revoked: "reauth",
  unauthorized_workspace: "reauth",
  unauthorized_project: "recoverable",
  auth_unavailable: "recoverable",
  malformed_frame: "recoverable",
  too_many_connections: "recoverable",
  broadcast_lagged: "transient",
  not_joined: "transient",
};

export function classifyError(code: string): ErrorSurface {
  return (SURFACE_BY_CODE as Record<string, ErrorSurface | undefined>)[code]
    ?? "recoverable";
}

export function isReauthCode(code: string): boolean {
  return classifyError(code) === "reauth";
}

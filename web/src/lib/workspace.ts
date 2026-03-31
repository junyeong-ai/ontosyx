"use client";

/**
 * Client-side workspace ID management.
 *
 * When the user selects a workspace (multi-tenant mode), the ID is stored
 * in sessionStorage and sent as X-Workspace-Id on every API request.
 *
 * When no workspace is selected, the header is omitted and the backend
 * falls back to the user's default workspace.
 */

const STORAGE_KEY = "ontosyx.workspace_id";
const NAME_KEY = "ontosyx.workspace_name";
const ROLE_KEY = "ontosyx.workspace_role";

/** Get the active workspace ID, or undefined to use the default. */
export function getWorkspaceId(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return window.sessionStorage.getItem(STORAGE_KEY) ?? undefined;
}

/** Set the active workspace ID. Pass undefined to clear (use default). */
export function setWorkspaceId(id: string | undefined): void {
  if (typeof window === "undefined") return;
  if (id) {
    window.sessionStorage.setItem(STORAGE_KEY, id);
  } else {
    window.sessionStorage.removeItem(STORAGE_KEY);
  }
  // Clear cached name/role when workspace changes
  window.sessionStorage.removeItem(NAME_KEY);
  window.sessionStorage.removeItem(ROLE_KEY);
}

/** Get the cached workspace name. */
export function getWorkspaceName(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return window.sessionStorage.getItem(NAME_KEY) ?? undefined;
}

/** Cache the workspace name in sessionStorage. */
export function setWorkspaceName(name: string | undefined): void {
  if (typeof window === "undefined") return;
  if (name) {
    window.sessionStorage.setItem(NAME_KEY, name);
  } else {
    window.sessionStorage.removeItem(NAME_KEY);
  }
}

/** Get the cached workspace role. */
export function getWorkspaceRole(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return window.sessionStorage.getItem(ROLE_KEY) ?? undefined;
}

/** Cache the workspace role in sessionStorage. */
export function setWorkspaceRole(role: string | undefined): void {
  if (typeof window === "undefined") return;
  if (role) {
    window.sessionStorage.setItem(ROLE_KEY, role);
  } else {
    window.sessionStorage.removeItem(ROLE_KEY);
  }
}

"use client";

/**
 * Client-side workspace ID management.
 *
 * Workspace preference persists in localStorage (survives tabs/sessions).
 * Sent as X-Workspace-Id on every API request via the API client.
 */

const STORAGE_KEY = "ontosyx.workspace_id";
const NAME_KEY = "ontosyx.workspace_name";
const ROLE_KEY = "ontosyx.workspace_role";

// One-time migration from sessionStorage → localStorage
function migrateFromSessionStorage(): void {
  if (typeof window === "undefined") return;
  const sessionId = window.sessionStorage.getItem(STORAGE_KEY);
  if (sessionId && !window.localStorage.getItem(STORAGE_KEY)) {
    window.localStorage.setItem(STORAGE_KEY, sessionId);
    const name = window.sessionStorage.getItem(NAME_KEY);
    if (name) window.localStorage.setItem(NAME_KEY, name);
    const role = window.sessionStorage.getItem(ROLE_KEY);
    if (role) window.localStorage.setItem(ROLE_KEY, role);
  }
  // Clean up sessionStorage regardless
  window.sessionStorage.removeItem(STORAGE_KEY);
  window.sessionStorage.removeItem(NAME_KEY);
  window.sessionStorage.removeItem(ROLE_KEY);
}

// Run migration on module load
if (typeof window !== "undefined") {
  migrateFromSessionStorage();
}

/** Get the active workspace ID, or undefined if not set. */
export function getWorkspaceId(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return window.localStorage.getItem(STORAGE_KEY) ?? undefined;
}

/** Set the active workspace ID. Pass undefined to clear. */
export function setWorkspaceId(id: string | undefined): void {
  if (typeof window === "undefined") return;
  if (id) {
    window.localStorage.setItem(STORAGE_KEY, id);
  } else {
    window.localStorage.removeItem(STORAGE_KEY);
    window.localStorage.removeItem(NAME_KEY);
    window.localStorage.removeItem(ROLE_KEY);
  }
}

/** Get the cached workspace name. */
export function getWorkspaceName(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return window.localStorage.getItem(NAME_KEY) ?? undefined;
}

/** Cache the workspace name. */
export function setWorkspaceName(name: string | undefined): void {
  if (typeof window === "undefined") return;
  if (name) window.localStorage.setItem(NAME_KEY, name);
  else window.localStorage.removeItem(NAME_KEY);
}

/** Get the cached workspace role. */
export function getWorkspaceRole(): string | undefined {
  if (typeof window === "undefined") return undefined;
  return window.localStorage.getItem(ROLE_KEY) ?? undefined;
}

/** Cache the workspace role. */
export function setWorkspaceRole(role: string | undefined): void {
  if (typeof window === "undefined") return;
  if (role) window.localStorage.setItem(ROLE_KEY, role);
  else window.localStorage.removeItem(ROLE_KEY);
}

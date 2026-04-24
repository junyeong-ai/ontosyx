"use client";

/**
 * Client-side workspace ID management.
 *
 * Sent as `X-Workspace-Id` on every API request via the API client.
 *
 * Two modes, by `NEXT_PUBLIC_OX_DEV_WORKSPACE_ID` presence:
 *
 *   Dev (`NEXT_PUBLIC_OX_DEV_WORKSPACE_ID` set by `dev.sh seed`):
 *       The env is the **single source of truth** — `dev.sh seed`
 *       regenerates the workspace on every boot, and a stale
 *       `localStorage.ontosyx.workspace_id` from a previous seed
 *       would otherwise shadow the fresh value and produce silent
 *       404s against workspace-scoped endpoints. The cache is kept
 *       in lock-step with the env so other modules reading
 *       localStorage directly (e.g. the workspace switcher UI) see
 *       the same value.
 *
 *   Production (env is undefined, branch DCE'd by Next.js):
 *       `localStorage.ontosyx.workspace_id` is authoritative.
 *       Populated by the login flow / workspace switcher; persists
 *       across tabs and sessions.
 */

const STORAGE_KEY = "ontosyx.workspace_id";
const NAME_KEY = "ontosyx.workspace_name";
const ROLE_KEY = "ontosyx.workspace_role";

/** Get the active workspace ID, or undefined if not set. */
export function getWorkspaceId(): string | undefined {
  if (typeof window === "undefined") return undefined;

  if (process.env.NODE_ENV !== "production") {
    const devWorkspace = process.env.NEXT_PUBLIC_OX_DEV_WORKSPACE_ID;
    if (devWorkspace) {
      // Mirror env → localStorage so any code that reads the cache
      // directly (or another tab opened before the most recent seed)
      // agrees with the dev anchor. Avoid a redundant write on the
      // common cache-hit path.
      const cached = window.localStorage.getItem(STORAGE_KEY);
      if (cached !== devWorkspace) {
        window.localStorage.setItem(STORAGE_KEY, devWorkspace);
      }
      return devWorkspace;
    }
  }

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

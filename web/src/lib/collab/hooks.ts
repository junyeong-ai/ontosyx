// React hooks for the collaboration WebSocket. The singleton lives
// at module scope so multiple components calling `useCollab()`
// share one socket; effects only manage lifecycle (init on mount,
// teardown on workspace switch).

"use client";

import { useEffect, useLayoutEffect, useRef } from "react";

import { clearWsTokenCache } from "./auth";
import { CollaborationClient } from "./client";
import { useCollabStore } from "./store";

let activeClient: CollaborationClient | null = null;
let activeWorkspaceId: string | null = null;

export interface UseCollabOptions {
  /** Absolute or path-relative WS URL. */
  url: string;
  workspaceId: string;
  /**
   * Token provider — called on every (re)connect. Callers don't
   * need to memoise this with `useCallback`; the hook reads
   * through a ref so a fresh closure on each render is fine.
   */
  getToken(): string | Promise<string>;
}

/**
 * Bind the calling component to the collaboration singleton.
 * Returns the client (or `null` on the first render before the
 * effect commits — UI code guards with `?.`).
 */
export function useCollab(opts: UseCollabOptions): CollaborationClient | null {
  const { url, workspaceId, getToken } = opts;
  const setConnectionState = useCollabStore((s) => s.setConnectionState);
  const applyServerMessage = useCollabStore((s) => s.applyServerMessage);
  const reset = useCollabStore((s) => s.reset);

  // Token provider may capture changing closure state on every
  // render; route it through a ref so the underlying socket only
  // re-creates when the workspace identity actually changes.
  const tokenProviderRef = useRef<UseCollabOptions["getToken"]>(getToken);
  useLayoutEffect(() => {
    tokenProviderRef.current = getToken;
  });

  useEffect(() => {
    // Empty workspaceId is the "not yet ready" / signed-out state
    // — tear down any active client and skip init. The hook resumes
    // automatically once the caller passes a real workspace id.
    if (!workspaceId) {
      if (activeClient) {
        activeClient.disconnect();
        activeClient = null;
        activeWorkspaceId = null;
        useCollabStore.getState().setClientReady(false);
        reset();
      }
      return;
    }

    // Tear down the previous client when the workspace switches.
    if (activeClient && activeWorkspaceId !== workspaceId) {
      activeClient.disconnect();
      activeClient = null;
      activeWorkspaceId = null;
      useCollabStore.getState().setClientReady(false);
      reset();
    }

    if (!activeClient) {
      activeClient = new CollaborationClient({
        url,
        workspaceId,
        // Read the latest provider through the ref — `getToken`
        // is captured stably here so token rotation (login
        // refresh, SessionRevoked recovery) works without
        // recreating the socket.
        getToken: () => tokenProviderRef.current(),
        onMessage: applyServerMessage,
        onStateChange: setConnectionState,
      });
      activeWorkspaceId = workspaceId;
      activeClient.connect();
      useCollabStore.getState().setClientReady(true);
    }
  }, [url, workspaceId, applyServerMessage, setConnectionState, reset]);

  return activeClient;
}

/**
 * Force-close the active collaboration client — call on sign-out.
 * Safe to call when no client is active. Also drops the cached
 * WS token so the next session can't reuse a stale mint.
 */
export function clearCollabClient(): void {
  if (activeClient) {
    activeClient.disconnect();
    activeClient = null;
    activeWorkspaceId = null;
  }
  useCollabStore.getState().reset();
  clearWsTokenCache();
}

/**
 * Auto-join `projectId` while mounted, leave on unmount or when
 * `projectId` changes. Gated on `clientReady` so the call queues
 * inside the client only after the singleton exists; the rejoin
 * set inside `CollaborationClient` then carries the room across
 * reconnects without further work from the caller.
 */
export function useCollabRoom(projectId: string | null | undefined): void {
  const clientReady = useCollabStore((s) => s.clientReady);

  useEffect(() => {
    if (!clientReady || !projectId || !activeClient) return;
    const client = activeClient;
    client.join(projectId);
    return () => {
      client.leave(projectId);
    };
  }, [clientReady, projectId]);
}

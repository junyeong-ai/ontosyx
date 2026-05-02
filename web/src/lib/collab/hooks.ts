// React hooks for the collaboration WebSocket. The singleton lives
// at module scope so multiple components calling `useCollab()`
// share one socket; effects only manage lifecycle (init on mount,
// teardown on workspace switch).

"use client";

import { useEffect } from "react";

import { CollaborationClient } from "./client";
import { useCollabStore } from "./store";

let activeClient: CollaborationClient | null = null;
let activeWorkspaceId: string | null = null;

export interface UseCollabOptions {
  /** Absolute or path-relative WS URL. */
  url: string;
  workspaceId: string;
  /** Token provider — see `CollaborationClient` for contract. */
  getToken(): Promise<string>;
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

  useEffect(() => {
    // Tear down the previous client when the workspace switches.
    if (activeClient && activeWorkspaceId !== workspaceId) {
      activeClient.disconnect();
      activeClient = null;
      activeWorkspaceId = null;
      reset();
    }

    if (!activeClient) {
      activeClient = new CollaborationClient({
        url,
        workspaceId,
        getToken,
        onMessage: applyServerMessage,
        onStateChange: setConnectionState,
      });
      activeWorkspaceId = workspaceId;
      activeClient.connect();
    }
  }, [url, workspaceId, getToken, applyServerMessage, setConnectionState, reset]);

  return activeClient;
}

/**
 * Force-close the active collaboration client — call on sign-out.
 * Safe to call when no client is active.
 */
export function clearCollabClient(): void {
  if (activeClient) {
    activeClient.disconnect();
    activeClient = null;
    activeWorkspaceId = null;
  }
  useCollabStore.getState().reset();
}

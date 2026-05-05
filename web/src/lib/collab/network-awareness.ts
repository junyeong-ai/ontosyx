"use client";

// Browser network-awareness for the collaboration client.
//
// `online` / `offline` and the Page Visibility API let us adjust
// the socket's behaviour to what the user is actually doing: don't
// keep retrying when the OS says the network is down (the next
// `online` event will trigger an immediate reconnect), and skip
// cursor publishes while the tab is hidden so we don't burn CPU
// or bandwidth painting cursors no one will see.

import { useEffect } from "react";

import type { CollaborationClient } from "./client";
import { useCollabStore } from "./store";

/**
 * Subscribe `client` to `online` / `offline` and visibilitychange
 * events. Returns nothing — the side effects live for the lifetime
 * of the component that calls the hook.
 */
export function useNetworkAwareness(client: CollaborationClient | null): void {
  useEffect(() => {
    if (!client) return;
    if (typeof window === "undefined") return;

    const onOnline = () => {
      // The browser came back online — kick the reconnect schedule
      // by closing whatever socket is parked. The client's existing
      // reconnect loop runs on the next microtask.
      if (client.connectionState() !== "ready") {
        client.connect();
      }
    };
    const onOffline = () => {
      // No-op — the next send/recv will fail naturally and the
      // reconnect loop already backs off. Keeping this listener
      // documents the intent and gives ops a single hook to extend
      // (e.g. surface an offline banner) without touching the
      // client class.
    };

    window.addEventListener("online", onOnline);
    window.addEventListener("offline", onOffline);
    return () => {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("offline", onOffline);
    };
  }, [client]);
}

/**
 * Track `document.visibilityState` in the store so call sites can
 * skip expensive work (cursor send, animation frames) while the
 * tab is hidden.
 */
export function useVisibilityAwareness(): void {
  const setHidden = useCollabStore((s) => s.setHidden);
  useEffect(() => {
    if (typeof document === "undefined") return;
    const update = () => setHidden(document.visibilityState === "hidden");
    update();
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, [setHidden]);
}

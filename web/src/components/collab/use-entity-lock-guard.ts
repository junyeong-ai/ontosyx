// `useEntityLockGuard` — declares "I am editing this entity for
// the lifetime of this component". Acquires on mount, refreshes
// on a timer to keep the lock alive while the inspector is open,
// releases on unmount.
//
// Renewal interval is half of `lock_ttl_secs` (default 300s on
// the server). Refreshing through the existing `acquire_lock`
// idempotent path keeps the wire surface narrow — no separate
// "renew" message — and the server treats every TTL bump as a
// caller-only signal so other members don't see the heartbeat.

"use client";

import { useEffect } from "react";

import { useCollabClient } from "./use-collab-client";

/**
 * Idempotent acquire interval (ms). Pairs with the server's
 * `lock_ttl_secs = 300` default; refreshing every 120s leaves
 * a 3× safety margin against clock skew + transient socket lag.
 */
const RENEWAL_INTERVAL_MS = 120_000;

export function useEntityLockGuard(
  projectId: string | undefined,
  entityId: string | undefined,
  enabled: boolean = true,
): void {
  const client = useCollabClient();

  useEffect(() => {
    if (!enabled || !client || !projectId || !entityId) return;

    client.acquireLock(projectId, entityId);
    const timer = setInterval(() => {
      client.acquireLock(projectId, entityId);
    }, RENEWAL_INTERVAL_MS);

    return () => {
      clearInterval(timer);
      client.releaseLock(projectId, entityId);
    };
  }, [client, projectId, entityId, enabled]);
}

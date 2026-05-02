// `useEntityLock` — read-only view of an entity's current lock
// state for a given project. Surfaces a discriminated union so
// inspectors / cards can branch on `unlocked`, `locked-by-me`, or
// `locked-by-other` without reaching into the raw store value.

"use client";

import { useMemo } from "react";

import { selectLockFor, useCollabStore } from "@/lib/collab";
import { useAuth } from "@/hooks/use-auth";

export type EntityLockStatus =
  | { kind: "unlocked" }
  | { kind: "locked-by-me"; expiresAt: string }
  | { kind: "locked-by-other"; heldBy: string; expiresAt: string };

export function useEntityLock(
  projectId: string | undefined,
  entityId: string | undefined,
): EntityLockStatus {
  const { user } = useAuth();
  const lock = useCollabStore(
    selectLockFor(projectId ?? "", entityId ?? ""),
  );
  return useMemo<EntityLockStatus>(() => {
    if (!projectId || !entityId || !lock) return { kind: "unlocked" };
    if (user?.sub && lock.heldBy === user.sub) {
      return { kind: "locked-by-me", expiresAt: lock.expiresAt };
    }
    return {
      kind: "locked-by-other",
      heldBy: lock.heldBy,
      expiresAt: lock.expiresAt,
    };
  }, [projectId, entityId, lock, user?.sub]);
}

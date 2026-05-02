// Reactive accessor for the active `CollaborationClient` singleton.
// `useCollab` constructs it at the workbench shell; this hook is
// the read-only counterpart UI components use to call
// `moveCursor` / `acquireLock` without owning the lifecycle.
//
// The singleton is module-level state inside `lib/collab/hooks`;
// the store's `clientReady` flag flips synchronously the moment
// the constructor runs, so deriving from it picks up the new
// instance without an effect → setState round-trip.

"use client";

import type { CollaborationClient } from "@/lib/collab";
import { getActiveCollabClient, useCollabStore } from "@/lib/collab";

export function useCollabClient(): CollaborationClient | null {
  const ready = useCollabStore((s) => s.clientReady);
  return ready ? getActiveCollabClient() : null;
}

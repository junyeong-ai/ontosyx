// RemoteCursorLayer — emits the current viewer's cursor and renders
// every other collaborator's cursor on top of the canvas. Drops in
// as a child of `<GraphCanvas>` so it lives inside the ReactFlow
// context and can use `screenToFlowPosition` /
// `flowToScreenPosition` for coordinate conversion.
//
// Cursors travel as world coordinates (zoom/pan invariant) so two
// users at different zoom levels still see each other in the right
// canvas-relative spot. The screen-pixel translation happens
// per-render at the receiver.

"use client";

import { useEffect, useMemo, useRef } from "react";
import { useReactFlow } from "@xyflow/react";

import {
  colorFor,
  selectCursors,
  selectHidden,
  selectPresence,
  useCollabStore,
} from "@/lib/collab";
import { useCollabClient } from "./use-collab-client";
import { cn } from "@/lib/cn";

/** Hard floor on cursor send interval. The hub also throttles at
 *  50ms by default; sending more often would just be drops at
 *  the server. Trimming on the client saves bandwidth + battery. */
const CURSOR_SEND_INTERVAL_MS = 60;

/** Idle cursors fade out after this long without an update. The
 *  store still has the last position, but at this point the user
 *  is probably AFK. */
const CURSOR_IDLE_FADE_MS = 30_000;

interface RemoteCursorLayerProps {
  projectId: string;
  /** The current viewer's user id — suppress their own cursor
   *  (the OS already renders it natively). */
  currentUserId: string | undefined;
}

export function RemoteCursorLayer({
  projectId,
  currentUserId,
}: RemoteCursorLayerProps) {
  return (
    <>
      <CursorEmitter projectId={projectId} />
      <CursorRenderer projectId={projectId} currentUserId={currentUserId} />
    </>
  );
}

// ---------------------------------------------------------------------------
// Send side — throttled mousemove → world coords → client.moveCursor
// ---------------------------------------------------------------------------

function CursorEmitter({ projectId }: { projectId: string }) {
  const { screenToFlowPosition } = useReactFlow();
  const client = useCollabClient();
  const hidden = useCollabStore(selectHidden);
  const lastSentRef = useRef(0);

  useEffect(() => {
    if (!client || hidden) return;
    const handler = (e: MouseEvent) => {
      const now = performance.now();
      if (now - lastSentRef.current < CURSOR_SEND_INTERVAL_MS) return;
      lastSentRef.current = now;
      const flow = screenToFlowPosition({ x: e.clientX, y: e.clientY });
      client.moveCursor(projectId, flow.x, flow.y, null);
    };
    window.addEventListener("mousemove", handler);
    return () => window.removeEventListener("mousemove", handler);
  }, [projectId, screenToFlowPosition, client, hidden]);

  return null;
}

// ---------------------------------------------------------------------------
// Receive side — store cursors → screen coords → render
// ---------------------------------------------------------------------------

interface RenderedCursor {
  userId: string;
  userName: string;
  x: number;
  y: number;
  color: string;
}

function CursorRenderer({
  projectId,
  currentUserId,
}: {
  projectId: string;
  currentUserId: string | undefined;
}) {
  const cursors = useCollabStore(selectCursors(projectId));
  const presence = useCollabStore(selectPresence(projectId));
  const { flowToScreenPosition } = useReactFlow();

  // user_id → user_name lookup. Presence is the source of truth
  // for naming; cursor frames carry it too but presence is a
  // smaller working set to memoise against.
  const nameById = useMemo(() => {
    const m = new Map<string, string>();
    for (const p of presence) m.set(p.user_id, p.user_name);
    return m;
  }, [presence]);

  const rendered: RenderedCursor[] = useMemo(() => {
    const out: RenderedCursor[] = [];
    for (const [userId, cursor] of cursors) {
      if (userId === currentUserId) continue;
      const userName = nameById.get(userId);
      if (!userName) continue; // user left the room mid-frame
      const screen = flowToScreenPosition({ x: cursor.x, y: cursor.y });
      out.push({
        userId,
        userName,
        x: screen.x,
        y: screen.y,
        color: colorFor(userId),
      });
    }
    return out;
  }, [cursors, nameById, currentUserId, flowToScreenPosition]);

  if (rendered.length === 0) return null;

  return (
    <div
      className="pointer-events-none fixed inset-0 z-30"
      aria-hidden
    >
      {rendered.map((c) => (
        <RemoteCursor key={c.userId} {...c} />
      ))}
    </div>
  );
}

interface RemoteCursorProps {
  userId: string;
  userName: string;
  x: number;
  y: number;
  color: string;
}

function RemoteCursor({ userName, x, y, color }: RemoteCursorProps) {
  return (
    <div
      className={cn(
        "absolute -translate-x-1 -translate-y-1 transition-[left,top] duration-100 ease-linear",
        "fade-in-0 animate-in",
      )}
      style={
        {
          left: `${x}px`,
          top: `${y}px`,
          // Inline custom property fades the cursor when its
          // `last-update` mark goes stale; consumers can override.
          "--cursor-color": color,
        } as React.CSSProperties
      }
    >
      <CursorArrow color={color} />
      <span
        className="ml-3 mt-1 inline-block whitespace-nowrap rounded-md px-1.5 py-0.5 text-2xs font-medium text-white shadow-sm"
        style={{ backgroundColor: color }}
      >
        {userName}
      </span>
    </div>
  );
}

function CursorArrow({ color }: { color: string }) {
  return (
    <svg
      width="14"
      height="18"
      viewBox="0 0 14 18"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style={{ filter: "drop-shadow(0 1px 2px rgba(0,0,0,0.3))" }}
    >
      <path
        d="M2 1L12 11L7 12L9 17L6 18L4 13L2 16V1Z"
        fill={color}
        stroke="white"
        strokeWidth="0.75"
        strokeLinejoin="round"
      />
    </svg>
  );
}

// Suppress the unused-fade timer for now — the design system's
// next pass will hook idle fade into a per-cursor `lastUpdate`
// timestamp tracked alongside the position.
void CURSOR_IDLE_FADE_MS;

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

import { useEffect, useMemo, useRef, useState } from "react";
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

/** Cursors stay fully opaque for this long after the most recent
 *  frame. Tuned to match a normal pause (sip of coffee, glance at
 *  a panel) without flickering. */
const CURSOR_VISIBLE_MS = 30_000;

/** After [`CURSOR_VISIBLE_MS`] cursors fade linearly to zero over
 *  this window — once the gradient runs out the cursor stops
 *  rendering entirely, but presence stays so the avatar in the
 *  header remains. */
const CURSOR_FADE_MS = 30_000;

/** Tick interval for the idle-fade re-evaluation. Cursors that
 *  haven't moved still need their opacity recomputed against the
 *  wall clock; one second is well below the perception threshold
 *  and runs cheaply for any plausible cursor count. */
const FADE_TICK_MS = 1_000;

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
  opacity: number;
}

/**
 * Linear fade from 1 → 0 between [`CURSOR_VISIBLE_MS`] and
 * `CURSOR_VISIBLE_MS + CURSOR_FADE_MS`. Past that the cursor
 * stops rendering; presence stays in the header so the user
 * isn't "gone" — just AFK.
 */
function idleOpacity(lastUpdateAt: number, now: number): number {
  const age = now - lastUpdateAt;
  if (age < CURSOR_VISIBLE_MS) return 1;
  if (age >= CURSOR_VISIBLE_MS + CURSOR_FADE_MS) return 0;
  return 1 - (age - CURSOR_VISIBLE_MS) / CURSOR_FADE_MS;
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

  // Idle-fade tick — re-evaluates opacity against the wall clock
  // even when no new cursor frames arrive. The state holds the
  // latest `Date.now()` so the render-time `useMemo` can read it
  // without calling the impure `Date.now()` itself. Lazy
  // initialiser captures `now` at first mount; `setInterval`
  // refreshes it from inside the effect.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), FADE_TICK_MS);
    return () => clearInterval(t);
  }, []);

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
      const opacity = idleOpacity(cursor.lastUpdateAt, now);
      if (opacity === 0) continue;
      const screen = flowToScreenPosition({ x: cursor.x, y: cursor.y });
      out.push({
        userId,
        userName,
        x: screen.x,
        y: screen.y,
        color: colorFor(userId),
        opacity,
      });
    }
    return out;
  }, [cursors, nameById, currentUserId, flowToScreenPosition, now]);

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
  opacity: number;
}

function RemoteCursor({ userName, x, y, color, opacity }: RemoteCursorProps) {
  return (
    <div
      className={cn(
        "absolute -translate-x-1 -translate-y-1 transition-[left,top,opacity] duration-200 ease-linear",
        "fade-in-0 animate-in",
      )}
      style={{
        left: `${x}px`,
        top: `${y}px`,
        opacity,
      }}
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


"use client";

import { useCallback, useState } from "react";

// ---------------------------------------------------------------------------
// useGraphContextMenu — position + target for a graph-canvas context menu
// ---------------------------------------------------------------------------
//
// Every graph surface (QueryCanvas, ExploreCanvas, future ACL inspectors)
// needs the same modal floating menu shape: remember the click coordinates,
// remember what was clicked, close on demand. Pulling that state into a
// reusable hook keeps the surfaces free of bespoke `useState` pairs and
// gives context-menu policy one place to evolve (accessibility fixes,
// analytics hooks, multi-select support).

export type GraphContextMenuTarget =
  | { type: "node"; id: string }
  | { type: "edge"; id: string };

export interface GraphContextMenuState {
  x: number;
  y: number;
  target: GraphContextMenuTarget;
}

export interface UseGraphContextMenuResult {
  state: GraphContextMenuState | null;
  /** Open the menu at `clientX`/`clientY` for the given target. */
  open: (event: MouseEvent | React.MouseEvent, target: GraphContextMenuTarget) => void;
  close: () => void;
}

export function useGraphContextMenu(): UseGraphContextMenuResult {
  const [state, setState] = useState<GraphContextMenuState | null>(null);

  const open = useCallback<UseGraphContextMenuResult["open"]>(
    (event, target) => {
      // Cancel the native browser menu so our own renders cleanly.
      event.preventDefault();
      setState({
        x: "clientX" in event ? event.clientX : 0,
        y: "clientY" in event ? event.clientY : 0,
        target,
      });
    },
    [],
  );

  const close = useCallback(() => setState(null), []);

  return { state, open, close };
}

"use client";

import { forwardRef, useCallback, useImperativeHandle, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import {
  ReactFlowProvider,
  useReactFlow,
  type Node,
  type Edge,
  type NodeProps,
  type EdgeProps,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  type OnNodesChange,
  type Viewport,
  BaseEdge,
  EdgeLabelRenderer,
  getSmoothStepPath,
  Handle,
  Position,
  applyNodeChanges,
  MarkerType,
} from "@xyflow/react";

import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import { arr } from "@/lib/ir-collections";
import { ContextMenu, type ContextMenuItem } from "@/components/workbench/canvas/context-menu";
import { GraphCanvas } from "@/components/workbench/canvas/graph-canvas";
import { useGraphInteractions } from "@/lib/use-graph-interactions";
import type { GraphContextMenuTarget } from "@/lib/use-graph-context-menu";
import type { PatternNode, PatternEdge } from "./ir-builder";

// ---------------------------------------------------------------------------
// QueryCanvas — XyFlow surface for building a query pattern visually
// ---------------------------------------------------------------------------
//
// Replaces the earlier DIV-based `pattern-canvas.tsx`. The move to XyFlow
// lets users position nodes freely, see real directed arrows between them,
// pan / zoom the workspace, and — on a follow-up commit — attach a
// context menu + keyboard shortcuts via a shared `useGraphInteractions`
// hook.
//
// The canvas remains a pure rendering surface. State ownership stays with
// `QueryBuilder`: every structural change (add / remove / drag) comes back
// through the prop callbacks so a single source of truth is preserved.
// Positions round-trip through `PatternNode.position`, so dragging a node
// and re-opening the canvas keeps the layout.

// ---------------------------------------------------------------------------
// Node/edge renderers
// ---------------------------------------------------------------------------

interface QueryNodeData extends Record<string, unknown> {
  label: string;
  alias: string;
  propCount: number;
  filterCount: number;
  selected: boolean;
  /** When `true`, the canvas paints a red ring around the node — the
   *  live validator found a blocking issue (missing label, unknown
   *  ontology entity, orphan filter, etc.). Independent of `selected`
   *  so an invalid selected node shows both rings. */
  hasError: boolean;
  onRemove: () => void;
  /** Pre-translated strings — next-intl hooks can't be called inside
   *  nodeTypes renderers registered with ReactFlow, so the parent passes
   *  them down through node data instead. */
  i18n: {
    meta: string;
    filters: string;
    removeAria: string;
    removeTitle: string;
  };
}

function QueryNodeRenderer({ data }: NodeProps & { data: QueryNodeData }) {
  // `hasError` wins over `selected` for border color so a selected
  // node with a validation issue still advertises the problem; the
  // secondary outline (ring) keeps the selection cue visible.
  const borderClass = data.hasError
    ? "border-red-500 bg-red-50 dark:border-red-500 dark:bg-red-950/30"
    : data.selected
      ? "border-emerald-500 bg-emerald-50 dark:border-emerald-600 dark:bg-emerald-950/30"
      : "border-zinc-200 hover:border-emerald-300 dark:border-zinc-700 dark:hover:border-emerald-700";

  return (
    <div
      className={`group/node relative cursor-pointer rounded-xl border-2 bg-white px-4 py-3 text-left shadow-sm transition-all dark:bg-zinc-800 ${borderClass} ${
        data.hasError && data.selected
          ? "ring-2 ring-emerald-400/60 ring-offset-1 ring-offset-white dark:ring-offset-zinc-900"
          : ""
      }`}
      aria-invalid={data.hasError || undefined}
    >
      <Handle type="target" position={Position.Left} className="!h-2 !w-2 !border-zinc-400 !bg-white" />
      <Handle type="source" position={Position.Right} className="!h-2 !w-2 !border-zinc-400 !bg-white" />
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          data.onRemove();
        }}
        aria-label={data.i18n.removeAria}
        className="absolute -right-1.5 -top-1.5 hidden h-5 w-5 items-center justify-center rounded-full bg-red-500 text-[10px] text-white shadow-sm group-hover/node:flex"
        title={data.i18n.removeTitle}
      >
        &times;
      </button>
      <div className="flex items-center gap-2">
        <div className="h-3 w-3 shrink-0 rounded-full bg-blue-400 dark:bg-blue-500" />
        <span className="text-xs font-semibold text-zinc-800 dark:text-zinc-200">
          {data.label}
        </span>
      </div>
      <div className="mt-1 text-[10px] text-muted-foreground">
        {data.i18n.meta}
        {data.filterCount > 0 && (
          <span className="ml-1 text-amber-500">{data.i18n.filters}</span>
        )}
      </div>
    </div>
  );
}

interface QueryEdgeData extends Record<string, unknown> {
  relType: string;
  alias: string;
  filterCount: number;
  selected: boolean;
  /** Same semantics as `QueryNodeData.hasError` — the live validator
   *  flagged this edge (missing relationship type, dangling endpoint,
   *  unknown ontology type, orphan filter). */
  hasError: boolean;
  onRemove: () => void;
  /** Pre-translated strings — see `QueryNodeData.i18n` for rationale. */
  i18n: {
    removeAria: string;
    removeTitle: string;
  };
}

function QueryEdgeRenderer(props: EdgeProps) {
  const { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd } = props;
  const data = props.data as QueryEdgeData | undefined;
  const [path, labelX, labelY] = getSmoothStepPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  const selected = data?.selected ?? false;
  const hasError = data?.hasError ?? false;
  // Error stroke beats selection; selection still shines through the
  // label pill (see below).
  const stroke = hasError ? "#ef4444" : selected ? "#f59e0b" : "#a1a1aa";
  const strokeWidth = hasError ? 2.5 : selected ? 2 : 1.5;

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        style={{ stroke, strokeWidth }}
      />
      <EdgeLabelRenderer>
        <div
          style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
          aria-invalid={hasError || undefined}
          className={`group/edge pointer-events-auto absolute flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors ${
            hasError
              ? "border-red-400 bg-red-50 text-red-700 dark:border-red-500 dark:bg-red-950/30 dark:text-red-300"
              : selected
                ? "border-amber-400 bg-amber-50 text-amber-700 dark:border-amber-600 dark:bg-amber-950/30 dark:text-amber-400"
                : "border-zinc-300 bg-white text-zinc-500 hover:border-amber-300 dark:border-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground"
          }`}
        >
          <span>{data?.relType}</span>
          {data?.filterCount ? (
            <span className="text-amber-500">&middot; {data.filterCount}</span>
          ) : null}
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              data?.onRemove();
            }}
            aria-label={data?.i18n.removeAria ?? ""}
            className="ml-0.5 hidden text-muted-foreground hover:text-red-500 group-hover/edge:inline dark:hover:text-red-400"
            title={data?.i18n.removeTitle ?? ""}
          >
            &times;
          </button>
        </div>
      </EdgeLabelRenderer>
    </>
  );
}

const nodeTypesRegistry = { query: QueryNodeRenderer };
const edgeTypesRegistry = { query: QueryEdgeRenderer };

// ---------------------------------------------------------------------------
// QueryCanvas
// ---------------------------------------------------------------------------

export interface QueryCanvasProps {
  nodes: PatternNode[];
  edges: PatternEdge[];
  nodeTypes: NodeTypeDef[];
  edgeTypes: EdgeTypeDef[];
  selectedId: string | null;
  onSelectNode: (nodeId: string | null) => void;
  onSelectEdge: (edgeId: string | null) => void;
  onAddNode: (nodeType: NodeTypeDef, position?: { x: number; y: number }) => void;
  onAddEdge: (edgeType: EdgeTypeDef) => void;
  onRemoveNode: (nodeId: string) => void;
  onRemoveEdge: (edgeId: string) => void;
  /** Persist a position update so `PatternNode.position` survives reloads. */
  onMoveNode: (nodeId: string, position: { x: number; y: number }) => void;
  /** Node/edge ids the live validator flagged with `severity: "error"`.
   *  Default is an empty set — callers without validation still render
   *  the canvas without any error chrome. */
  errorIds?: ReadonlySet<string>;
}

/**
 * Imperative handle exposing the canvas's viewport. Used by the query
 * builder to snapshot the current zoom / pan into a saved pattern and
 * restore it when a pattern is loaded. Keeps XyFlow's ReactFlow
 * provider encapsulated inside the canvas (the parent never has to
 * import `useReactFlow`).
 */
export interface QueryCanvasHandle {
  getViewport: () => Viewport;
  setViewport: (viewport: Viewport) => void;
}

/**
 * Default grid position for a freshly added node whose drop coordinates
 * weren't provided (e.g. user clicked the palette item instead of dragging).
 * Nodes are laid out in a simple 3-column grid until the user re-arranges.
 */
function defaultGridPosition(index: number): { x: number; y: number } {
  const col = index % 3;
  const row = Math.floor(index / 3);
  return { x: 80 + col * 220, y: 40 + row * 140 };
}

/**
 * Singleton empty set reused for every render when the caller doesn't
 * supply `errorIds`. Reusing the same identity keeps `useMemo`
 * dependencies stable across renders.
 */
const EMPTY_ERROR_IDS: ReadonlySet<string> = new Set();

const QueryCanvasInner = forwardRef<QueryCanvasHandle, QueryCanvasProps>(function QueryCanvasInner(
  props,
  ref,
) {
  const {
    nodes,
    edges,
    nodeTypes,
    selectedId,
    onSelectNode,
    onSelectEdge,
    onAddNode,
    onAddEdge,
    onRemoveNode,
    onRemoveEdge,
    onMoveNode,
    errorIds,
  } = props;

  const t = useTranslations("workbench.queryBuilder.canvas");

  // Stable empty set so memoised flowNodes / flowEdges don't see a new
  // identity on every render when the caller omits `errorIds`.
  const effectiveErrorIds: ReadonlySet<string> = errorIds ?? EMPTY_ERROR_IDS;

  const reactFlow = useReactFlow();
  const wrapperRef = useRef<HTMLDivElement | null>(null);

  // Expose viewport getters/setters to the parent so it can round-trip
  // zoom + pan through saved patterns. Depends on `useReactFlow`, which
  // requires us to be inside the `ReactFlowProvider` — hence the handle
  // is wired here rather than on the outer `QueryCanvas` export.
  useImperativeHandle(
    ref,
    () => ({
      getViewport: () => reactFlow.getViewport(),
      setViewport: (vp: Viewport) => reactFlow.setViewport(vp),
    }),
    [reactFlow],
  );

  // Derive the shared interaction policy (context menu + Delete/Backspace +
  // Escape) from the canvas's own selection state. `selectedTarget` is
  // `null` whenever the user hasn't selected anything — the hook then
  // makes Delete/Backspace a no-op.
  const selectedTarget = useMemo<GraphContextMenuTarget | null>(() => {
    if (!selectedId) return null;
    if (nodes.some((n) => n.id === selectedId)) return { type: "node", id: selectedId };
    if (edges.some((e) => e.id === selectedId)) return { type: "edge", id: selectedId };
    return null;
  }, [selectedId, nodes, edges]);

  const clearSelection = useCallback(() => {
    onSelectNode(null);
    onSelectEdge(null);
  }, [onSelectNode, onSelectEdge]);

  const { contextMenu } = useGraphInteractions({
    selectedTarget,
    onClearSelection: clearSelection,
    onRemoveNode,
    onRemoveEdge,
  });
  /**
   * Positions assigned on the fly for nodes that arrive without one.
   * Not persisted — `onMoveNode` flushes to state the first time the
   * user drags the node, at which point the fallback is no longer read.
   */
  const [fallbackPositions, setFallbackPositions] = useState<Record<string, { x: number; y: number }>>({});

  const flowNodes: Node[] = useMemo(() => {
    return nodes.map((n, idx) => {
      const nt = nodeTypes.find((nodeType) => nodeType.label === n.label);
      const position =
        n.position ?? fallbackPositions[n.id] ?? defaultGridPosition(idx);
      const propCount = arr(nt?.properties).length;
      return {
        id: n.id,
        type: "query",
        position,
        data: {
          label: n.label,
          alias: n.alias,
          propCount,
          filterCount: n.filters.length,
          selected: selectedId === n.id,
          hasError: effectiveErrorIds.has(n.id),
          onRemove: () => onRemoveNode(n.id),
          i18n: {
            meta: t("nodeMeta", { alias: n.alias, count: propCount }),
            filters: t("nodeFilters", { count: n.filters.length }),
            removeAria: t("removeNodeAria", { label: n.label }),
            removeTitle: t("removeNodeTitle"),
          },
        } satisfies QueryNodeData,
      };
    });
  }, [nodes, nodeTypes, selectedId, fallbackPositions, onRemoveNode, effectiveErrorIds, t]);

  const flowEdges: Edge[] = useMemo(() => {
    return edges.map((e) => ({
      id: e.id,
      source: e.sourceNodeId,
      target: e.targetNodeId,
      type: "query",
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
      data: {
        relType: e.relType,
        alias: e.alias,
        filterCount: e.filters.length,
        selected: selectedId === e.id,
        hasError: effectiveErrorIds.has(e.id),
        onRemove: () => onRemoveEdge(e.id),
        i18n: {
          removeAria: t("removeEdgeAria", { label: e.relType }),
          removeTitle: t("removeEdgeTitle"),
        },
      } satisfies QueryEdgeData,
    }));
  }, [edges, selectedId, onRemoveEdge, effectiveErrorIds, t]);

  // ----- Drag/drop from palette ----------------------------------------

  const handleDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();
      const nodeData = event.dataTransfer.getData("application/qb-node");
      const edgeData = event.dataTransfer.getData("application/qb-edge");
      if (nodeData) {
        try {
          const nt = JSON.parse(nodeData) as NodeTypeDef;
          const bounds = wrapperRef.current?.getBoundingClientRect();
          const position = bounds
            ? reactFlow.screenToFlowPosition({
                x: event.clientX - bounds.left,
                y: event.clientY - bounds.top,
              })
            : undefined;
          onAddNode(nt, position);
        } catch {
          /* ignore parse errors */
        }
      } else if (edgeData) {
        try {
          const et = JSON.parse(edgeData) as EdgeTypeDef;
          onAddEdge(et);
        } catch {
          /* ignore parse errors */
        }
      }
    },
    [onAddNode, onAddEdge, reactFlow],
  );

  const handleDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
  }, []);

  // ----- Selection + movement -----------------------------------------

  const handleNodeClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      onSelectNode(selectedId === node.id ? null : node.id);
    },
    [onSelectNode, selectedId],
  );

  const handleEdgeClick: EdgeMouseHandler = useCallback(
    (_event, edge) => {
      onSelectEdge(selectedId === edge.id ? null : edge.id);
    },
    [onSelectEdge, selectedId],
  );

  const handleNodesChange: OnNodesChange = useCallback(
    (changes) => {
      // Let XyFlow compute the next render-time positions (used when
      // we feed them back into the Node list), and persist drag-end
      // positions to the caller's state so reloads keep the layout.
      const next = applyNodeChanges(changes, flowNodes);
      const byId = new Map(next.map((n) => [n.id, n.position]));
      setFallbackPositions((prev) => {
        const updated = { ...prev };
        for (const [id, pos] of byId.entries()) {
          updated[id] = pos;
        }
        return updated;
      });
      for (const change of changes) {
        if (change.type === "position" && !change.dragging && change.position) {
          onMoveNode(change.id, change.position);
        }
      }
    },
    [flowNodes, onMoveNode],
  );

  const handlePaneClick = useCallback(() => {
    onSelectNode(null);
    onSelectEdge(null);
    contextMenu.close();
  }, [onSelectNode, onSelectEdge, contextMenu]);

  const handleNodeContextMenu: NodeMouseHandler = useCallback(
    (event, node) => {
      onSelectNode(node.id);
      contextMenu.open(event, { type: "node", id: node.id });
    },
    [contextMenu, onSelectNode],
  );

  const handleEdgeContextMenu: EdgeMouseHandler = useCallback(
    (event, edge) => {
      onSelectEdge(edge.id);
      contextMenu.open(event, { type: "edge", id: edge.id });
    },
    [contextMenu, onSelectEdge],
  );

  // Context-menu items: derived fresh each render so they capture the
  // latest targetId. `Remove` is the only action right now; Edit-style
  // actions (rename variable, convert direction) can slot in here
  // without touching the hook or the surface.
  const contextMenuItems = useMemo<ContextMenuItem[]>(() => {
    const target = contextMenu.state?.target;
    if (!target) return [];
    return [
      {
        label:
          target.type === "node"
            ? t("contextRemoveNode")
            : t("contextRemoveEdge"),
        shortcut: "Del",
        danger: true,
        onClick: () => {
          if (target.type === "node") {
            onRemoveNode(target.id);
          } else {
            onRemoveEdge(target.id);
          }
          clearSelection();
        },
      },
    ];
  }, [contextMenu.state, onRemoveNode, onRemoveEdge, clearSelection, t]);

  // ----- Empty state ---------------------------------------------------

  if (nodes.length === 0) {
    return (
      <div
        ref={wrapperRef}
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        className="flex h-full flex-col items-center justify-center gap-3 rounded-lg border-2 border-dashed border-zinc-200 p-8 text-center dark:border-zinc-700"
      >
        <div className="flex h-12 w-12 items-center justify-center rounded-full bg-zinc-100 dark:bg-zinc-800">
          <svg
            className="h-5 w-5 text-muted-foreground"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
        </div>
        <p className="text-sm font-medium text-zinc-600 dark:text-muted-foreground">
          {t("emptyTitle")}
        </p>
        <p className="text-xs text-muted-foreground">
          {t("emptyHint")}
        </p>
      </div>
    );
  }

  return (
    <div
      ref={wrapperRef}
      onDrop={handleDrop}
      onDragOver={handleDragOver}
      className="relative h-full overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-700"
    >
      <GraphCanvas
        nodes={flowNodes}
        edges={flowEdges}
        onNodesChange={handleNodesChange}
        onNodeClick={handleNodeClick}
        onEdgeClick={handleEdgeClick}
        onPaneClick={handlePaneClick}
        onNodeContextMenu={handleNodeContextMenu}
        onEdgeContextMenu={handleEdgeContextMenu}
        nodeTypes={nodeTypesRegistry}
        edgeTypes={edgeTypesRegistry}
        minZoom={0.2}
        className="bg-zinc-50/50 dark:bg-zinc-900/50"
      />
      {contextMenu.state && contextMenuItems.length > 0 && (
        <ContextMenu
          state={{
            type: contextMenu.state.target.type,
            id: contextMenu.state.target.id,
            x: contextMenu.state.x,
            y: contextMenu.state.y,
          }}
          items={contextMenuItems}
          onClose={contextMenu.close}
        />
      )}
    </div>
  );
});

/**
 * Public entry point — wraps the canvas in a ReactFlowProvider so
 * `useReactFlow` (for `screenToFlowPosition` on drops) can hook into
 * the instance. The provider is cheap; each QueryCanvas gets its own.
 *
 * Forwards `ref` to the inner canvas so callers can snapshot / restore
 * the XyFlow viewport (see `QueryCanvasHandle`).
 */
export const QueryCanvas = forwardRef<QueryCanvasHandle, QueryCanvasProps>(function QueryCanvas(
  props,
  ref,
) {
  return (
    <ReactFlowProvider>
      <QueryCanvasInner {...props} ref={ref} />
    </ReactFlowProvider>
  );
});

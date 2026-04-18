"use client";

import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { useAppStore } from "@/lib/store";
import { executeFromIr } from "@/lib/api/queries";
import type { NodeTypeDef, EdgeTypeDef, PropertyDef, QueryResult } from "@/types/api";
import { PatternPalette, type PaletteTab } from "./pattern-palette";
import { QueryCanvas, type QueryCanvasHandle } from "./query-canvas";
import { FilterEditor } from "./filter-editor";
import { ReturnSelector } from "./return-selector";
import {
  buildQueryIR,
  previewCypher,
  type PatternNode,
  type PatternEdge,
  type PatternFilter,
  type PatternReturnField,
  type PatternOrderClause,
} from "./ir-builder";
import { useSuggestions, type Suggestion } from "./use-suggestions";
import { SavedPatternsMenu } from "./saved-patterns-menu";
import { toPatternIR, fromPatternIR } from "./saved-pattern-io";
import { WidgetRenderer } from "@/components/widgets/widget-renderer";
import { normalizeQueryResult } from "@/lib/api";
import type { SavedPattern } from "@/lib/api/queries";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Read-only banner labels
// ---------------------------------------------------------------------------
// Maps the backend's `ReadOnlyReason.original_op` (Rust `QueryOp`
// variant name, stable and canonical) to a human label for the
// locked-canvas banner. The wire value stays untouched — this is a
// UI-only lookup, and a future variant will appear as its own key
// here without requiring a backend change.
const READ_ONLY_REASON_LABELS: Record<string, string> = {
  Match: "Match query",
  PathFind: "Path-find query",
  Aggregate: "Aggregate query",
  Union: "UNION query",
  Chain: "Chained query",
  CallSubquery: "Subquery (CALL)",
  Mutate: "Mutation (CREATE / MERGE / DELETE)",
  Analytics: "Analytics query",
};

// ---------------------------------------------------------------------------
// QueryBuilder — Main container for visual query building
// ---------------------------------------------------------------------------

export function QueryBuilder() {
  const ontology = useAppStore((s) => s.ontology);
  const savedOntologyId = useAppStore((s) => s.savedOntologyId);

  // Pattern state
  const [nodes, setNodes] = useState<PatternNode[]>([]);
  const [edges, setEdges] = useState<PatternEdge[]>([]);
  const [returnFields, setReturnFields] = useState<PatternReturnField[]>([]);
  const [orderBy, setOrderBy] = useState<PatternOrderClause[]>([]);
  const [limit, setLimit] = useState<number | null>(25);

  // Selection state
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Execution state
  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [compiledCypher, setCompiledCypher] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Config panel (right side)
  const [configTab, setConfigTab] = useState<"filter" | "return">("filter");

  // Ontology types
  const nodeTypes = ontology?.node_types ?? [];
  const edgeTypes = ontology?.edge_types ?? [];

  // Counter for alias generation
  const [nodeCounter, setNodeCounter] = useState(0);
  const [edgeCounter, setEdgeCounter] = useState(0);

  // Backend id of the currently-loaded saved pattern. `null` after a
  // fresh New / Clear — Save then prompts for a name rather than
  // updating in place.
  const [currentPatternId, setCurrentPatternId] = useState<string | null>(null);

  // `readOnlyReason.original_op` from the backend's `decompile` when
  // the saved pattern came from a non-`Match` QueryIR (Aggregate /
  // Union / Chain / PathFind / Mutate / Analytics / CallSubquery).
  // Non-null pins the canvas into a read-only state — all edit
  // affordances (palette drops, filter edits, Clear, Save As, Run)
  // remain disabled, and a banner names the operation kind so the
  // user can open the source query text elsewhere instead.
  //
  // Clear / New resets it; loading a Match pattern also resets it
  // (the wire field is absent in that case).
  const [readOnlyReason, setReadOnlyReason] = useState<string | null>(null);

  // Baseline serialised canvas state captured at the last Save / Load.
  // Used as the comparand for the "unsaved changes" dot — we memoise
  // `JSON.stringify(currentSnapshot)` against this and render the dot
  // when they diverge. O(N) compare per render, but `useMemo` gates
  // recomputation to the state slices that actually affect the shape.
  const savedBaselineRef = useRef<string | null>(null);

  // Imperative handle on the canvas — used to snapshot / restore the
  // XyFlow viewport (zoom + pan) when saving / loading patterns.
  const canvasRef = useRef<QueryCanvasHandle>(null);

  // Pending viewport — written on Load, consumed once by a layout
  // effect after React renders the new node set. Replaces the fragile
  // requestAnimationFrame-then-setViewport pattern so viewport restore
  // waits for XyFlow to know its bounds (larger patterns need >1 frame).
  const pendingViewportRef = useRef<
    { zoom: number; x: number; y: number } | null
  >(null);

  // Palette tab state
  const [paletteTab, setPaletteTab] = useState<PaletteTab>("nodes");
  const prevTabRef = useRef<PaletteTab>("nodes");

  // Derive selected node label for suggestions
  const selectedNode_ = nodes.find((n) => n.id === selectedId);
  const selectedNodeLabel = selectedNode_?.label ?? null;

  // Smart suggestions
  const suggestions = useSuggestions(selectedNodeLabel, nodes, ontology);

  // Auto-switch to "Suggested" tab when a node is selected with suggestions
  useEffect(() => {
    if (selectedNodeLabel && suggestions.length > 0) {
      if (paletteTab !== "suggested") {
        prevTabRef.current = paletteTab;
      }
      setPaletteTab("suggested");
    } else if (!selectedNodeLabel && paletteTab === "suggested") {
      setPaletteTab(prevTabRef.current);
    }
  }, [selectedNodeLabel, suggestions.length]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---------------------------------------------------------------------------
  // Handlers
  // ---------------------------------------------------------------------------

  const handleAddNode = useCallback(
    (nt: NodeTypeDef, position?: { x: number; y: number }) => {
      const alias = `n${nodeCounter}`;
      const newNode: PatternNode = {
        id: `pn-${Date.now()}`,
        label: nt.label,
        alias,
        filters: [],
        position,
      };
      setNodes((prev) => [...prev, newNode]);
      setNodeCounter((c) => c + 1);
      setSelectedId(newNode.id);
    },
    [nodeCounter],
  );

  const handleMoveNode = useCallback(
    (nodeId: string, position: { x: number; y: number }) => {
      setNodes((prev) =>
        prev.map((n) => (n.id === nodeId ? { ...n, position } : n)),
      );
    },
    [],
  );

  const handleAddEdge = useCallback(
    (et: EdgeTypeDef) => {
      // Auto-create source/target nodes if they don't exist in the pattern
      let srcNode = nodes.find((n) => {
        const nt = nodeTypes.find((t) => t.label === n.label);
        return nt?.id === et.source_node_id;
      });
      let tgtNode = nodes.find((n) => {
        const nt = nodeTypes.find((t) => t.label === n.label);
        return nt?.id === et.target_node_id && n !== srcNode;
      });

      const newNodes = [...nodes];
      let nc = nodeCounter;

      if (!srcNode) {
        const srcType = nodeTypes.find((t) => t.id === et.source_node_id);
        if (!srcType) return;
        srcNode = {
          id: `pn-${Date.now()}-src`,
          label: srcType.label,
          alias: `n${nc}`,
          filters: [],
        };
        newNodes.push(srcNode);
        nc++;
      }

      if (!tgtNode) {
        const tgtType = nodeTypes.find((t) => t.id === et.target_node_id);
        if (!tgtType) return;
        tgtNode = {
          id: `pn-${Date.now()}-tgt`,
          label: tgtType.label,
          alias: `n${nc}`,
          filters: [],
        };
        newNodes.push(tgtNode);
        nc++;
      }

      // Duplicate edge check
      if (edges.some((e) => e.sourceNodeId === srcNode.id && e.targetNodeId === tgtNode.id && e.relType === et.label)) {
        toast("Edge already exists");
        return;
      }

      const alias = `r${edgeCounter}`;
      const newEdge: PatternEdge = {
        id: `pe-${Date.now()}`,
        sourceNodeId: srcNode.id,
        targetNodeId: tgtNode.id,
        relType: et.label,
        alias,
        filters: [],
      };

      setNodes(newNodes);
      setNodeCounter(nc);
      setEdges((prev) => [...prev, newEdge]);
      setEdgeCounter((c) => c + 1);
      setSelectedId(newEdge.id);
    },
    [nodes, edges, nodeTypes, nodeCounter, edgeCounter],
  );

  const handleAddSuggestion = useCallback(
    (suggestion: Suggestion) => {
      const { edge, direction, targetNode } = suggestion;

      // Check if target node already exists in pattern
      let existingTarget = nodes.find((n) => n.label === targetNode.label);
      const newNodes = [...nodes];
      let nc = nodeCounter;

      if (!existingTarget) {
        existingTarget = {
          id: `pn-${Date.now()}-sug`,
          label: targetNode.label,
          alias: `n${nc}`,
          filters: [],
        };
        newNodes.push(existingTarget);
        nc++;
      }

      // Determine source and target for the edge based on direction
      const currentNode = nodes.find((n) => n.id === selectedId);
      if (!currentNode) return;

      const srcNodeId =
        direction === "outgoing" ? currentNode.id : existingTarget.id;
      const tgtNodeId =
        direction === "outgoing" ? existingTarget.id : currentNode.id;

      // Duplicate edge check
      if (edges.some((e) => e.sourceNodeId === srcNodeId && e.targetNodeId === tgtNodeId && e.relType === edge.label)) {
        toast("Edge already exists");
        return;
      }

      const alias = `r${edgeCounter}`;
      const newEdge: PatternEdge = {
        id: `pe-${Date.now()}-sug`,
        sourceNodeId: srcNodeId,
        targetNodeId: tgtNodeId,
        relType: edge.label,
        alias,
        filters: [],
      };

      setNodes(newNodes);
      setNodeCounter(nc);
      setEdges((prev) => [...prev, newEdge]);
      setEdgeCounter((c) => c + 1);
      // Select the target node to enable chain exploration
      setSelectedId(existingTarget.id);
    },
    [nodes, edges, selectedId, nodeCounter, edgeCounter],
  );

  const handleRemoveNode = useCallback(
    (nodeId: string) => {
      setNodes((prev) => prev.filter((n) => n.id !== nodeId));
      // Remove edges connected to this node
      setEdges((prev) =>
        prev.filter((e) => e.sourceNodeId !== nodeId && e.targetNodeId !== nodeId),
      );
      // Clean up return fields referencing this node's alias
      const removedNode = nodes.find((n) => n.id === nodeId);
      if (removedNode) {
        setReturnFields((prev) =>
          prev.filter((f) => f.alias !== removedNode.alias),
        );
        setOrderBy((prev) =>
          prev.filter((o) => o.alias !== removedNode.alias),
        );
      }
      if (selectedId === nodeId) setSelectedId(null);
    },
    [nodes, selectedId],
  );

  const handleRemoveEdge = useCallback(
    (edgeId: string) => {
      const removedEdge = edges.find((e) => e.id === edgeId);
      setEdges((prev) => prev.filter((e) => e.id !== edgeId));
      if (removedEdge) {
        setReturnFields((prev) =>
          prev.filter((f) => f.alias !== removedEdge.alias),
        );
        setOrderBy((prev) =>
          prev.filter((o) => o.alias !== removedEdge.alias),
        );
      }
      if (selectedId === edgeId) setSelectedId(null);
    },
    [edges, selectedId],
  );

  const handleUpdateFilters = useCallback(
    (filters: PatternFilter[]) => {
      if (!selectedId) return;
      setNodes((prev) =>
        prev.map((n) => (n.id === selectedId ? { ...n, filters } : n)),
      );
      setEdges((prev) =>
        prev.map((e) => (e.id === selectedId ? { ...e, filters } : e)),
      );
    },
    [selectedId],
  );

  // ---------------------------------------------------------------------------
  // Selected element info
  // ---------------------------------------------------------------------------

  const selectedNode = nodes.find((n) => n.id === selectedId);
  const selectedEdge = edges.find((e) => e.id === selectedId);
  const selectedElement = selectedNode ?? selectedEdge;

  const selectedProperties: PropertyDef[] = useMemo(() => {
    if (selectedNode) {
      const nt = nodeTypes.find((t) => t.label === selectedNode.label);
      return nt?.properties ?? [];
    }
    if (selectedEdge) {
      const et = edgeTypes.find((t) => t.label === selectedEdge.relType);
      return et?.properties ?? [];
    }
    return [];
  }, [selectedNode, selectedEdge, nodeTypes, edgeTypes]);

  // ---------------------------------------------------------------------------
  // Preview
  // ---------------------------------------------------------------------------

  const cypherPreview = useMemo(() => {
    if (nodes.length === 0) return "";
    return previewCypher({ nodes, edges, returnFields, orderBy, limit });
  }, [nodes, edges, returnFields, orderBy, limit]);

  // ---------------------------------------------------------------------------
  // Execute
  // ---------------------------------------------------------------------------

  const validatePattern = useCallback((): string | null => {
    if (nodes.length === 0) return "Add at least one node to the pattern";
    for (const edge of edges) {
      if (!nodes.find((n) => n.id === edge.sourceNodeId))
        return `Edge ${edge.relType} has invalid source`;
      if (!nodes.find((n) => n.id === edge.targetNodeId))
        return `Edge ${edge.relType} has invalid target`;
    }
    const seen = new Set<string>();
    for (const edge of edges) {
      const key = `${edge.sourceNodeId}-${edge.relType}-${edge.targetNodeId}`;
      if (seen.has(key)) return `Duplicate edge: ${edge.relType}`;
      seen.add(key);
    }
    return null;
  }, [nodes, edges]);

  const handleRun = useCallback(async () => {
    const validationError = validatePattern();
    if (validationError) {
      toast.error(validationError);
      return;
    }

    // Auto-generate return fields if none selected (return all node aliases)
    const effectiveReturnFields =
      returnFields.length > 0
        ? returnFields
        : nodes.map((n) => ({
            alias: n.alias,
            property: "*",
            aggregation: null,
          }));

    setIsRunning(true);
    setError(null);
    setResult(null);

    try {
      const ir = buildQueryIR({ nodes, edges, returnFields: effectiveReturnFields, orderBy, limit });
      const res = await executeFromIr(ir, savedOntologyId ?? undefined);
      setCompiledCypher(res.compiled_query ?? null);
      const normalized = normalizeQueryResult(res.result) ?? {
        columns: res.result.columns,
        rows: [],
      };
      setResult(normalized);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Query execution failed");
    } finally {
      setIsRunning(false);
    }
  }, [nodes, edges, returnFields, orderBy, limit, savedOntologyId, validatePattern]);

  const handleClear = useCallback(() => {
    setNodes([]);
    setEdges([]);
    setReturnFields([]);
    setOrderBy([]);
    setLimit(25);
    setSelectedId(null);
    setResult(null);
    setCompiledCypher(null);
    setError(null);
    setNodeCounter(0);
    setEdgeCounter(0);
    setCurrentPatternId(null);
    setReadOnlyReason(null);
    savedBaselineRef.current = null;
  }, []);

  // ---------------------------------------------------------------------------
  // Saved pattern IO — thread between builder state and backend PatternIR
  // ---------------------------------------------------------------------------

  const snapshotPatternIR = useCallback(
    () => {
      // Capture the current viewport so zoom + pan survive a reload.
      // `getViewport` is only available after the canvas has mounted;
      // an unmounted ref safely collapses to `undefined` via the
      // optional chain, which toPatternIR treats as "no layout hints".
      const vp = canvasRef.current?.getViewport();
      const pattern_ir = toPatternIR(
        { nodes, edges, returnFields, orderBy, limit },
        vp
          ? { layoutHints: { zoom: vp.zoom, pan_x: vp.x, pan_y: vp.y } }
          : {},
      );
      // Record the baseline so the "unsaved changes" dot clears on
      // the next render — the caller will update currentPatternId
      // before that render happens.
      savedBaselineRef.current = JSON.stringify(pattern_ir);
      return {
        pattern_ir,
        fallbackName: nodes[0]?.label
          ? `${nodes[0].label} pattern`
          : "Untitled pattern",
      };
    },
    [nodes, edges, returnFields, orderBy, limit],
  );

  const applyLoadedPattern = useCallback((pattern: SavedPattern) => {
    const { visual, layoutHints, readOnlyReason: reason } = fromPatternIR(
      pattern.pattern_ir as never,
    );
    setNodes(visual.nodes);
    setEdges(visual.edges);
    setReturnFields(visual.returnFields);
    setOrderBy(visual.orderBy);
    setLimit(visual.limit);
    setSelectedId(null);
    setResult(null);
    setCompiledCypher(null);
    setError(null);
    // Wire field comes through verbatim. `undefined` → fully editable
    // canvas; a string locks the canvas into read-only mode with that
    // QueryOp variant surfaced in the banner.
    setReadOnlyReason(reason?.original_op ?? null);
    // Alias generator starts above the max existing alias so new nodes
    // don't collide with loaded ones.
    const aliasNum = (alias: string) => {
      const m = alias.match(/^[a-z]+(\d+)$/);
      return m ? Number(m[1]) : 0;
    };
    const maxNode = visual.nodes.reduce((m, n) => Math.max(m, aliasNum(n.alias)), -1);
    const maxEdge = visual.edges.reduce((m, e) => Math.max(m, aliasNum(e.alias)), -1);
    setNodeCounter(maxNode + 1);
    setEdgeCounter(maxEdge + 1);
    setCurrentPatternId(pattern.id);
    // Stage the viewport for the effect below — restoring here would
    // race the pending `setNodes` render.
    if (
      layoutHints.zoom !== undefined ||
      layoutHints.pan_x !== undefined ||
      layoutHints.pan_y !== undefined
    ) {
      pendingViewportRef.current = {
        zoom: layoutHints.zoom ?? 1,
        x: layoutHints.pan_x ?? 0,
        y: layoutHints.pan_y ?? 0,
      };
    }
    // The loaded state *is* the saved baseline — no unsaved dot
    // until the user touches something.
    savedBaselineRef.current = JSON.stringify(pattern.pattern_ir);
    toast.success(`Loaded "${pattern.name}"`);
  }, []);

  // One-shot viewport apply: consumes the ref after the canvas has
  // rendered the new node set and XyFlow has computed its bounds.
  useEffect(() => {
    const pending = pendingViewportRef.current;
    if (!pending || nodes.length === 0) return;
    pendingViewportRef.current = null;
    canvasRef.current?.setViewport(pending);
  }, [nodes]);

  // "Unsaved changes" indicator — baseline vs current. Cheap via
  // JSON.stringify (<10ms for 100-node patterns); only re-computed
  // when the pattern state slices change.
  const isDirty = useMemo(() => {
    if (savedBaselineRef.current === null) return false;
    // We can't call snapshotPatternIR here (it writes the baseline)
    // so inline the payload shape. `toPatternIR` is pure; cheap.
    const vp = canvasRef.current?.getViewport();
    const current = JSON.stringify(
      toPatternIR(
        { nodes, edges, returnFields, orderBy, limit },
        vp
          ? { layoutHints: { zoom: vp.zoom, pan_x: vp.x, pan_y: vp.y } }
          : {},
      ),
    );
    return current !== savedBaselineRef.current;
  }, [nodes, edges, returnFields, orderBy, limit]);

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  if (!ontology) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-center">
        <div>
          <p className="text-sm font-medium text-zinc-600 dark:text-zinc-400">
            No ontology loaded
          </p>
          <p className="mt-1 text-xs text-zinc-400">
            Design or load an ontology first, then switch to Analyze mode.
          </p>
        </div>
      </div>
    );
  }

  // Run Query is enabled when: has nodes AND (has return fields OR
  // auto-return all) AND the pattern is editable (a non-Match
  // decompile is shown as read-only — its PatternIR collapsed to an
  // empty-nodes marker on the backend, so `nodes.length === 0` anyway,
  // but we gate explicitly so a hypothetical future UX that loads the
  // raw source text alongside can't accidentally execute the marker).
  const canRun = nodes.length > 0 && readOnlyReason === null;

  // Human-readable label for the locked-query banner. Keep the
  // mapping here rather than in the wire type so the UI can be
  // localised in one place; the wire stays canonical (Rust variant
  // names).
  const readOnlyLabel = readOnlyReason
    ? READ_ONLY_REASON_LABELS[readOnlyReason] ?? readOnlyReason
    : null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Toolbar */}
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-zinc-200 px-3 dark:border-zinc-800">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
          Query Builder
        </span>
        <div className="flex items-center gap-2">
          {nodes.length > 0 && returnFields.length === 0 && (
            <span className="text-[9px] text-amber-500">
              Select Return fields for custom projection, or run to return all
            </span>
          )}
          <SavedPatternsMenu
            ontologyId={ontology?.id ?? null}
            currentId={currentPatternId}
            getSnapshot={snapshotPatternIR}
            onLoad={applyLoadedPattern}
            onCurrentIdCleared={() => setCurrentPatternId(null)}
            onSaved={(saved) => setCurrentPatternId(saved.id)}
            onNewPattern={handleClear}
            disabled={nodes.length === 0}
            isDirty={isDirty}
          />
          <button
            onClick={handleClear}
            disabled={nodes.length === 0}
            className="rounded px-2 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-zinc-100 disabled:opacity-40 dark:hover:bg-zinc-800"
          >
            Clear
          </button>
          <button
            onClick={handleRun}
            disabled={isRunning || !canRun}
            className="rounded bg-emerald-600 px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
          >
            {isRunning ? "Running..." : "Run Query"}
          </button>
        </div>
      </div>

      {/* Read-only banner — surfaces when a non-Match QueryIR was
          decompiled into this PatternIR. The backend collapses such
          patterns into an empty-nodes marker plus a `readOnlyReason`,
          which we pass through verbatim. */}
      {readOnlyLabel && (
        <div
          role="alert"
          className="flex shrink-0 items-center gap-2 border-b border-amber-300 bg-amber-50 px-3 py-1.5 text-[11px] text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
        >
          <span className="font-semibold">Read-only:</span>
          <span>
            {readOnlyLabel} can’t be edited on the canvas. Clear to
            start a new query, or use the chat to modify it.
          </span>
        </div>
      )}

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left: Palette */}
        <div className="w-52 shrink-0 border-r border-zinc-200 dark:border-zinc-800">
          <PatternPalette
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            onAddNode={handleAddNode}
            onAddEdge={handleAddEdge}
            suggestions={suggestions}
            selectedNodeLabel={selectedNodeLabel}
            onAddSuggestion={handleAddSuggestion}
            activeTab={paletteTab}
            onTabChange={setPaletteTab}
          />
        </div>

        {/* Center: Canvas + Preview + Results */}
        <div className="flex min-h-0 flex-1 flex-col">
          {/* Canvas */}
          <div className="min-h-[240px] flex-1 overflow-hidden p-3">
            <QueryCanvas
              ref={canvasRef}
              nodes={nodes}
              edges={edges}
              nodeTypes={nodeTypes}
              edgeTypes={edgeTypes}
              selectedId={selectedId}
              onSelectNode={(id) => setSelectedId(id)}
              onSelectEdge={(id) => setSelectedId(id)}
              onAddNode={handleAddNode}
              onAddEdge={handleAddEdge}
              onRemoveNode={handleRemoveNode}
              onRemoveEdge={handleRemoveEdge}
              onMoveNode={handleMoveNode}
            />
          </div>

          {/* Cypher preview */}
          {cypherPreview && (
            <div className="shrink-0 border-t border-zinc-200 dark:border-zinc-800">
              <div className="flex items-center justify-between px-3 py-1">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                  Preview
                </span>
                <button
                  onClick={() => navigator.clipboard.writeText(cypherPreview)}
                  className="cursor-pointer text-[10px] text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
                >
                  Copy
                </button>
              </div>
              <pre className="max-h-24 overflow-auto bg-zinc-900 px-3 py-2 text-[11px] font-mono leading-relaxed text-emerald-400">
                {cypherPreview}
              </pre>
            </div>
          )}

          {/* Error */}
          {error && (
            <div className="shrink-0 border-t border-red-200 bg-red-50 px-3 py-2 dark:border-red-900 dark:bg-red-950/30">
              <p className="text-xs text-red-600 dark:text-red-400">{error}</p>
            </div>
          )}

          {/* Results */}
          {result && (
            <div className="shrink-0 border-t border-zinc-200 dark:border-zinc-800">
              <div className="flex items-center justify-between px-3 py-1">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                  Results ({result.rows.length} rows)
                </span>
                {compiledCypher && (
                  <span className="max-w-xs truncate text-[10px] text-zinc-400">
                    {compiledCypher}
                  </span>
                )}
              </div>
              <div className="max-h-64 overflow-auto p-2">
                <WidgetRenderer
                  spec={{ widget_type: "auto" }}
                  data={{
                    ...result,
                    metadata: { rows_returned: result.rows.length },
                  }}
                />
              </div>
            </div>
          )}
        </div>

        {/* Right: Config panel */}
        <div className="w-60 shrink-0 overflow-auto border-l border-zinc-200 dark:border-zinc-800">
          {selectedElement ? (
            <div className="p-3">
              <div className="mb-3">
                <span className="text-xs font-semibold text-zinc-700 dark:text-zinc-300">
                  {selectedNode ? selectedNode.label : selectedEdge?.relType}
                </span>
                <span className="ml-2 text-[10px] text-zinc-400">
                  ({selectedElement.alias})
                </span>
              </div>

              {/* Config tabs */}
              <div className="mb-3 flex border-b border-zinc-200 dark:border-zinc-800">
                <button
                  onClick={() => setConfigTab("filter")}
                  className={`px-3 py-1.5 text-xs font-medium transition-colors ${
                    configTab === "filter"
                      ? "border-b-2 border-emerald-600 text-emerald-600 dark:border-emerald-400 dark:text-emerald-400"
                      : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400"
                  }`}
                >
                  Filters
                </button>
                <button
                  onClick={() => setConfigTab("return")}
                  className={`px-3 py-1.5 text-xs font-medium transition-colors ${
                    configTab === "return"
                      ? "border-b-2 border-emerald-600 text-emerald-600 dark:border-emerald-400 dark:text-emerald-400"
                      : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400"
                  }`}
                >
                  Return
                </button>
              </div>

              {configTab === "filter" && (
                <FilterEditor
                  properties={selectedProperties}
                  filters={selectedElement.filters}
                  onChange={handleUpdateFilters}
                />
              )}

              {configTab === "return" && (
                <ReturnSelector
                  patternNodes={nodes}
                  patternEdges={edges}
                  nodeTypes={nodeTypes}
                  edgeTypes={edgeTypes}
                  returnFields={returnFields}
                  onReturnFieldsChange={setReturnFields}
                  orderBy={orderBy}
                  onOrderByChange={setOrderBy}
                  limit={limit}
                  onLimitChange={setLimit}
                />
              )}
            </div>
          ) : (
            <div className="flex h-full flex-col items-center justify-center p-4 text-center">
              <p className="text-xs text-zinc-400">
                Select a node or edge to configure filters and return fields.
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

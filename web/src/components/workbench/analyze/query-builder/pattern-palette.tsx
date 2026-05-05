"use client";

import { useState, useCallback } from "react";
import { useTranslations } from "next-intl";
import { FormInput } from "@/components/ui/form-input";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import type { Suggestion } from "./use-suggestions";
import { arr } from "@/lib/ir-collections";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

// ---------------------------------------------------------------------------
// PatternPalette — Available node/edge types from ontology
// ---------------------------------------------------------------------------

export type PaletteTab = "nodes" | "edges" | "suggested";

interface PatternPaletteProps {
  nodeTypes: NodeTypeDef[];
  edgeTypes: EdgeTypeDef[];
  onAddNode: (nodeType: NodeTypeDef) => void;
  onAddEdge: (edgeType: EdgeTypeDef) => void;
  suggestions?: Suggestion[];
  selectedNodeLabel?: string | null;
  onAddSuggestion?: (suggestion: Suggestion) => void;
  activeTab?: PaletteTab;
  onTabChange?: (tab: PaletteTab) => void;
}

export function PatternPalette({
  nodeTypes,
  edgeTypes,
  onAddNode,
  onAddEdge,
  suggestions = [],
  selectedNodeLabel = null,
  onAddSuggestion,
  activeTab,
  onTabChange,
}: PatternPaletteProps) {
  const t = useTranslations("workbench.queryBuilder.palette");
  const localeChain = useLocaleChain();
  const [search, setSearch] = useState("");
  const [internalTab, setInternalTab] = useState<PaletteTab>("nodes");

  const tab = activeTab ?? internalTab;
  const setTab = onTabChange ?? setInternalTab;

  const lowerSearch = search.toLowerCase();

  const filteredNodes = nodeTypes.filter(
    (nt) =>
      nt.label.toLowerCase().includes(lowerSearch) ||
      localize(nt.description, localeChain).toLowerCase().includes(lowerSearch),
  );

  const filteredEdges = edgeTypes.filter(
    (et) =>
      et.label.toLowerCase().includes(lowerSearch) ||
      localize(et.description, localeChain).toLowerCase().includes(lowerSearch),
  );

  const handleDragStartNode = useCallback(
    (e: React.DragEvent, nodeType: NodeTypeDef) => {
      e.dataTransfer.setData("application/qb-node", JSON.stringify(nodeType));
      e.dataTransfer.effectAllowed = "copy";
    },
    [],
  );

  const handleDragStartEdge = useCallback(
    (e: React.DragEvent, edgeType: EdgeTypeDef) => {
      e.dataTransfer.setData("application/qb-edge", JSON.stringify(edgeType));
      e.dataTransfer.effectAllowed = "copy";
    },
    [],
  );

  return (
    <div className="flex h-full flex-col">
      {/* Search */}
      <div className="shrink-0 p-2">
        <FormInput
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t("searchPlaceholder")}
          density="compact"
        />
      </div>

      {/* Tabs */}
      <div className="flex shrink-0 border-b border-divider px-2">
        <button type="button"
          onClick={() => setTab("nodes")}
          className={`px-3 py-1.5 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
            tab === "nodes"
              ? "border-b-2 border-brand-foreground text-brand-foreground"
              : "text-foreground-muted hover:text-foreground-muted"
          }`}
        >
          {t("tabNodes", { count: filteredNodes.length })}
        </button>
        <button type="button"
          onClick={() => setTab("edges")}
          className={`px-3 py-1.5 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
            tab === "edges"
              ? "border-b-2 border-brand-foreground text-brand-foreground"
              : "text-foreground-muted hover:text-foreground-muted"
          }`}
        >
          {t("tabEdges", { count: filteredEdges.length })}
        </button>
        {selectedNodeLabel && (
          <button type="button"
            onClick={() => setTab("suggested")}
            className={`px-3 py-1.5 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
              tab === "suggested"
                ? "border-b-2 border-concept-foreground text-concept-foreground"
                : "text-foreground-muted hover:text-foreground-muted"
            }`}
          >
            {t("tabSuggested", { count: suggestions.length })}
          </button>
        )}
      </div>

      {/* List */}
      <div className="flex-1 overflow-auto p-2 space-y-1">
        {tab === "nodes" &&
          filteredNodes.map((nt) => {
            const description = localize(nt.description, localeChain);
            return (
              <div
                key={nt.id}
                draggable
                onDragStart={(e) => handleDragStartNode(e, nt)}
                onClick={() => onAddNode(nt)}
                className="group cursor-grab rounded-lg border border-divider bg-surface-base px-3 py-2 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:bg-brand-surface active:cursor-grabbing"
              >
                <div className="flex items-center gap-2">
                  <div className="h-2.5 w-2.5 shrink-0 rounded-full bg-info-foreground" />
                  <span className="text-xs font-medium text-foreground">
                    {nt.label}
                  </span>
                </div>
                {description && (
                  <p className="mt-0.5 text-2xs text-foreground-muted line-clamp-1">
                    {description}
                  </p>
                )}
                <div className="mt-1 text-2xs text-foreground-muted">
                  {t("propertiesCount", { count: arr(nt.properties).length })}
                </div>
              </div>
            );
          })}

        {tab === "edges" &&
          filteredEdges.map((et) => {
            const srcLabel =
              nodeTypes.find((n) => n.id === et.source_node_id)?.label ?? "?";
            const tgtLabel =
              nodeTypes.find((n) => n.id === et.target_node_id)?.label ?? "?";
            return (
              <div
                key={et.id}
                draggable
                onDragStart={(e) => handleDragStartEdge(e, et)}
                onClick={() => onAddEdge(et)}
                className="group cursor-grab rounded-lg border border-divider bg-surface-base px-3 py-2 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:bg-brand-surface active:cursor-grabbing"
              >
                <div className="flex items-center gap-2">
                  <div className="h-2.5 w-2.5 shrink-0 rounded-sm bg-warning-foreground" />
                  <span className="text-xs font-medium text-foreground">
                    {et.label}
                  </span>
                </div>
                <p className="mt-0.5 text-2xs text-foreground-muted">
                  {srcLabel} &rarr; {tgtLabel}
                </p>
              </div>
            );
          })}

        {tab === "nodes" && filteredNodes.length === 0 && (
          <p className="py-4 text-center text-xs text-foreground-muted">
            {t("noNodeTypes")}
          </p>
        )}
        {tab === "edges" && filteredEdges.length === 0 && (
          <p className="py-4 text-center text-xs text-foreground-muted">
            {t("noEdgeTypes")}
          </p>
        )}

        {tab === "suggested" && !selectedNodeLabel && (
          <p className="py-4 text-center text-xs text-foreground-muted">
            {t("selectNodeForSuggestions")}
          </p>
        )}

        {tab === "suggested" &&
          selectedNodeLabel &&
          suggestions.map((s, i) => (
            <div
              key={`${s.edge.id}-${s.direction}-${i}`}
              onClick={() => onAddSuggestion?.(s)}
              className="group cursor-pointer rounded-lg border border-divider bg-surface-base px-3 py-2 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-concept-border hover:bg-concept-surface"
            >
              <div className="flex items-center gap-2">
                <span className="shrink-0 text-2xs text-foreground-muted">
                  {s.direction === "outgoing" ? "\u2192" : "\u2190"}
                </span>
                <span className="text-xs font-medium text-foreground">
                  {s.edge.label}
                </span>
                <span
                  className={`ms-auto h-2 w-2 shrink-0 rounded-full ${
                    s.alreadyInPattern
                      ? "bg-brand-solid"
                      : "bg-info-foreground"
                  }`}
                />
              </div>
              <p className="mt-0.5 text-2xs text-foreground-muted">
                {s.direction === "outgoing" ? "\u2192 " : "\u2190 "}
                {s.targetNode.label}
              </p>
            </div>
          ))}

        {tab === "suggested" &&
          selectedNodeLabel &&
          suggestions.length === 0 && (
            <p className="py-4 text-center text-xs text-foreground-muted">
              {t("noRelatedEdges")}
            </p>
          )}
      </div>
    </div>
  );
}

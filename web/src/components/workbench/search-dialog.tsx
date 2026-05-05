"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { FocusTrap } from "@/components/ui/focus-trap";
import { Search, X } from "lucide-react";
import { searchGraph } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import { SearchInput } from "@/components/ui/form-input";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/lib/store";
import { useImeAwareInput } from "@/hooks/use-ime-aware-input";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import {
  type SearchResultNode,
  toSearchResultNodes,
  resolveDisplayName,
  resolveSubtitle,
} from "./explore/graph-utils";
import { arr } from "@/lib/ir-collections";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

// ---------------------------------------------------------------------------
// SearchDialog — Cmd+K graph entity search overlay
// ---------------------------------------------------------------------------

// --- Schema match types ---

interface SchemaNodeMatch {
  kind: "node";
  node: NodeTypeDef;
  matchReason: string;
}

interface SchemaEdgeMatch {
  kind: "edge";
  edge: EdgeTypeDef;
  sourceLabel: string;
  targetLabel: string;
  matchReason: string;
}

type SchemaMatch = SchemaNodeMatch | SchemaEdgeMatch;

// --- Schema search (local, instant) ---

function searchSchema(
  query: string,
  ontology: { node_types: NodeTypeDef[]; edge_types: EdgeTypeDef[] },
  chain: readonly string[],
): SchemaMatch[] {
  const q = query.toLowerCase();
  if (!q) return [];

  const nodeLabelMap = new Map<string, string>();
  for (const n of ontology.node_types) {
    nodeLabelMap.set(n.id, n.label);
  }

  const results: SchemaMatch[] = [];

  for (const node of ontology.node_types) {
    if (node.label.toLowerCase().includes(q)) {
      results.push({ kind: "node", node, matchReason: "label" });
    } else if (arr(node.properties).some((p) => p.name.toLowerCase().includes(q))) {
      const matchingProp = arr(node.properties).find((p) => p.name.toLowerCase().includes(q));
      results.push({ kind: "node", node, matchReason: `property: ${matchingProp?.name}` });
    } else if (localize(node.description, chain).toLowerCase().includes(q)) {
      results.push({ kind: "node", node, matchReason: "description" });
    }
  }

  for (const edge of ontology.edge_types) {
    const sourceLabel = nodeLabelMap.get(edge.source_node_id) ?? "?";
    const targetLabel = nodeLabelMap.get(edge.target_node_id) ?? "?";
    if (edge.label.toLowerCase().includes(q)) {
      results.push({ kind: "edge", edge, sourceLabel, targetLabel, matchReason: "label" });
    } else if (sourceLabel.toLowerCase().includes(q) || targetLabel.toLowerCase().includes(q)) {
      results.push({ kind: "edge", edge, sourceLabel, targetLabel, matchReason: "endpoint" });
    } else if (arr(edge.properties).some((p) => p.name.toLowerCase().includes(q))) {
      const matchingProp = arr(edge.properties).find((p) => p.name.toLowerCase().includes(q));
      results.push({ kind: "edge", edge, sourceLabel, targetLabel, matchReason: `property: ${matchingProp?.name}` });
    }
  }

  return results;
}

export function SearchDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTranslations("workbench.searchDialog");
  const inputRef = useRef<HTMLInputElement>(null);
  // IME-aware input: Hangul composition is only propagated as `query` once
  // the user finishes composing a syllable (so searches don't fire on
  // intermediate jamo like "ㅎ").
  const searchInput = useImeAwareInput("");
  const query = searchInput.committedValue;
  const setQuery = searchInput.setValue;
  const [dataHits, setDataHits] = useState<SearchResultNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [dataSearched, setDataSearched] = useState(false);
  const [selectedIdx, setSelectedIdx] = useState(0);

  const selectOne = useAppStore((s) => s.selectOne);
  const ontology = useAppStore((s) => s.ontology);
  const localeChain = useLocaleChain();

  // Instant schema search as user types
  const schemaMatches = useMemo(() => {
    if (!ontology || !query.trim()) return [];
    return searchSchema(query.trim(), ontology, localeChain);
  }, [query, ontology, localeChain]);

  // Total selectable items count
  const totalItems = schemaMatches.length + dataHits.length;

  // Focus input when opened
  useEffect(() => {
    if (open) {
      setQuery("");
      setDataHits([]);
      setDataSearched(false);
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open, setQuery]);

  // Reset selected index when results change
  useEffect(() => {
    setSelectedIdx(0);
  }, []);

  const runDataSearch = useCallback(async (q: string) => {
    if (!q.trim()) return;
    setLoading(true);
    setDataSearched(true);
    try {
      const result = await searchGraph(q.trim());
      setDataHits(toSearchResultNodes(result));
    } catch {
      setDataHits([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const handleSelectSchema = useCallback((match: SchemaMatch) => {
    if (match.kind === "node") {
      selectOne({ kind: "node", id: match.node.id });
    } else {
      selectOne({ kind: "edge", id: match.edge.id });
    }
    if (!useAppStore.getState().isInspectorOpen) {
      useAppStore.getState().toggleInspector();
    }
    onClose();
  }, [selectOne, onClose]);

  const handleSelectData = useCallback((hit: SearchResultNode) => {
    // Find matching ontology node by label and select it on canvas
    const ont = useAppStore.getState().ontology;
    if (ont) {
      const matchLabel = hit.labels[0];
      const node = arr(ont.node_types).find((n) => n.label === matchLabel);
      if (node) {
        selectOne({ kind: "node", id: node.id });
        if (!useAppStore.getState().isInspectorOpen) {
          useAppStore.getState().toggleInspector();
        }
      }
    }
    onClose();
  }, [selectOne, onClose]);

  const handleSelectByIndex = useCallback((idx: number) => {
    if (idx < schemaMatches.length) {
      handleSelectSchema(schemaMatches[idx]);
    } else {
      const dataIdx = idx - schemaMatches.length;
      if (dataIdx < dataHits.length) {
        handleSelectData(dataHits[dataIdx]);
      }
    }
  }, [schemaMatches, dataHits, handleSelectSchema, handleSelectData]);

  if (!open) return null;

  const hasQuery = query.trim().length > 0;
  const hasSchemaResults = schemaMatches.length > 0;
  const hasDataResults = dataHits.length > 0;

  return (
    <FocusTrap
      focusTrapOptions={{
        initialFocus: false,
        allowOutsideClick: true,
        escapeDeactivates: false,
      }}
    >
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t("dialogAria")}
      className="fixed inset-0 z-modal flex items-start justify-center pt-[15vh]"
    >
      {/* Backdrop as a real <button type="button"> gives mouse + keyboard close behavior
          without relying on global keyDown handlers at the document level. */}
      <button
        type="button"
        aria-label={t("closeAria")}
        className="absolute inset-0 cursor-default bg-surface-scrim-soft"
        onClick={onClose}
      />
      <div
        className="relative w-full max-w-lg rounded-xl border border-divider bg-surface-base shadow-4"
      >
        {/* Search input */}
        <div className="border-b border-divider p-2.5">
          <SearchInput
            ref={inputRef}
            type="search"
            value={searchInput.value}
            onChange={searchInput.bind.onChange}
            onCompositionStart={searchInput.bind.onCompositionStart}
            onCompositionEnd={searchInput.bind.onCompositionEnd}
            onKeyDown={(e) => {
              // Ignore key events while the IME is composing — the browser
              // fires `Enter`/keyCode 229 during Hangul commit which would
              // otherwise double-trigger a search.
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Escape") {
                onClose();
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelectedIdx((i) => Math.min(i + 1, totalItems - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelectedIdx((i) => Math.max(i - 1, 0));
              } else if (e.key === "Enter" && !loading) {
                e.preventDefault();
                if (totalItems > 0) {
                  handleSelectByIndex(selectedIdx);
                } else if (!dataSearched) {
                  runDataSearch(query);
                }
              }
            }}
            placeholder={t("placeholder")}
            aria-label={t("placeholder")}
            leadingIcon={Search}
            trailing={
              <>
                {loading && <Spinner size="xs" className="text-foreground-muted" />}
                <button type="button"
                  onClick={onClose}
                  className="text-foreground-muted hover:text-foreground"
                  aria-label={t("closeAria")}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </>
            }
          />
        </div>

        {/* Results */}
        <div className="max-h-80 overflow-auto">
          {/* Empty state */}
          {!hasQuery && (
            <p className="px-4 py-6 text-center text-xs text-foreground-muted">
              {t("emptyHint")}
            </p>
          )}

          {/* Typing hint: has query, no results yet */}
          {hasQuery && !hasSchemaResults && !dataSearched && !loading && (
            <p className="px-4 py-6 text-center text-xs text-foreground-muted">
              {t("noSchemaMatches")}
            </p>
          )}

          {/* Schema results section */}
          {hasSchemaResults && (
            <div>
              <div className="px-3 py-1.5 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("schemaMatches")}
              </div>
              {schemaMatches.map((match, i) => {
                const isSelected = i === selectedIdx;
                if (match.kind === "node") {
                  const propCount = arr(match.node.properties).length;
                  const constraintCount = match.node.constraints?.length ?? 0;
                  return (
                    <button type="button"
                      key={`schema-node-${match.node.id}`}
                      onClick={() => handleSelectSchema(match)}
                      onMouseEnter={() => setSelectedIdx(i)}
                      className={cn(
                        "flex w-full items-center gap-2 px-4 py-1.5 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                        isSelected
                          ? "bg-brand-surface"
                          : "hover:bg-surface-raised",
                      )}
                    >
                      <span className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground-strong">
                        {t("nodeBadge")}
                      </span>
                      <span className="flex-1 truncate text-xs font-medium text-foreground-strong">
                        {match.node.label}
                      </span>
                      <span className="text-2xs text-foreground-muted">
                        {t("propCount", { count: propCount })}
                        {constraintCount > 0 && t("constraintExtra", { count: constraintCount })}
                      </span>
                    </button>
                  );
                } else {
                  return (
                    <button type="button"
                      key={`schema-edge-${match.edge.id}`}
                      onClick={() => handleSelectSchema(match)}
                      onMouseEnter={() => setSelectedIdx(i)}
                      className={cn(
                        "flex w-full items-center gap-2 px-4 py-1.5 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                        isSelected
                          ? "bg-brand-surface"
                          : "hover:bg-surface-raised",
                      )}
                    >
                      <span className="rounded bg-info-surface px-1.5 py-0.5 text-2xs font-medium text-info-foreground">
                        {t("edgeBadge")}
                      </span>
                      <span className="flex-1 truncate text-xs text-foreground-strong">
                        <span className="text-foreground-muted">{match.sourceLabel}</span>
                        {" → "}
                        <span className="font-medium">{match.edge.label}</span>
                        {" → "}
                        <span className="text-foreground-muted">{match.targetLabel}</span>
                      </span>
                    </button>
                  );
                }
              })}
            </div>
          )}

          {/* Data search hint — shown when schema results exist but data not yet searched */}
          {hasQuery && hasSchemaResults && !dataSearched && !loading && (
            <div className="border-t border-divider-soft px-4 py-2 text-center text-2xs text-foreground-muted">
              {t("pressEnterForData")}
            </div>
          )}

          {/* Data results section */}
          {dataSearched && (
            <div className={cn(hasSchemaResults && "border-t border-divider-soft")}>
              <div className="px-3 py-1.5 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("dataMatches")}
                {loading && <Spinner size="xs" className="ms-1 inline-block text-foreground-muted" />}
              </div>
              {!loading && !hasDataResults && (
                <p className="px-4 py-3 text-center text-2xs text-foreground-muted">
                  {t("noDataResults", { query })}
                </p>
              )}
              {dataHits.map((hit, i) => {
                const globalIdx = schemaMatches.length + i;
                const isSelected = globalIdx === selectedIdx;
                return (
                  <button type="button"
                    key={hit.elementId || `data-${i}`}
                    onClick={() => handleSelectData(hit)}
                    onMouseEnter={() => setSelectedIdx(globalIdx)}
                    className={cn(
                      "flex w-full items-start gap-3 px-4 py-2 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                      isSelected
                        ? "bg-brand-surface"
                        : "hover:bg-surface-raised",
                    )}
                  >
                    <div className="flex flex-wrap gap-1 pt-0.5">
                      {hit.labels.map((l) => (
                        <span
                          key={l}
                          className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground"
                        >
                          {l}
                        </span>
                      ))}
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-xs font-medium text-foreground-strong">
                        {resolveDisplayName(hit.props)}
                      </p>
                      <p className="truncate text-2xs text-foreground-muted">
                        {resolveSubtitle(hit.props)}
                      </p>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer hint */}
        <div className="flex items-center gap-3 border-t border-divider px-3 py-1.5 text-2xs text-foreground-muted">
          <span><KeyboardShortcut keys="Enter" variant="outline" /> {t("footer.searchData")}</span>
          <span><KeyboardShortcut glyph="↑↓" variant="outline" /> {t("footer.navigate")}</span>
          <span><KeyboardShortcut keys="Escape" variant="outline" /> {t("footer.close")}</span>
          <span className="ms-auto text-foreground-muted">{t("footer.hint")}</span>
        </div>
      </div>
    </div>
    </FocusTrap>
  );
}

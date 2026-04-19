"use client";

import { useCallback, useRef } from "react";
import { useTranslations } from "next-intl";
import type { Node } from "@xyflow/react";

import { useClickOutside } from "@/lib/use-click-outside";
import { exportCanvasImage } from "./canvas-helpers";
import { PerspectiveSwitcher } from "./perspective-switcher";
import type { ExportFormat } from "@/lib/export-utils";

interface CanvasToolbarProps {
  nodes: Node[];
  ontologyName: string;
  topologySignature: string;
  isExportOpen: boolean;
  setIsExportOpen: (v: boolean | ((prev: boolean) => boolean)) => void;
  onExportSchema: (format: ExportFormat) => Promise<void>;
  onApplyPositions: (positions: Record<string, { x: number; y: number }>) => void;
}

/**
 * Top-right canvas toolbar: perspective switcher + image/schema export.
 */
export function CanvasToolbar({
  nodes,
  ontologyName,
  topologySignature,
  isExportOpen,
  setIsExportOpen,
  onExportSchema,
  onApplyPositions,
}: CanvasToolbarProps) {
  const t = useTranslations("workbench.canvas.toolbar");
  const exportRef = useRef<HTMLDivElement>(null);
  const closeExport = useCallback(() => setIsExportOpen(false), [setIsExportOpen]);
  useClickOutside(exportRef, closeExport, isExportOpen);

  return (
    <div className="absolute right-2 top-2 z-10 flex flex-wrap items-center justify-end gap-1.5">
      <PerspectiveSwitcher
        nodes={nodes}
        topologySignature={topologySignature}
        onApplyPositions={onApplyPositions}
        onOpen={() => setIsExportOpen(false)}
      />
      <div ref={exportRef} className="relative">
        <button
          onClick={() => setIsExportOpen((v) => !v)}
          className="flex items-center rounded-md border border-zinc-200 bg-white px-2 py-1 text-[10px] font-medium text-zinc-600 shadow-sm transition-colors hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-muted-foreground dark:hover:bg-zinc-800"
        >
          {t("export")}
        </button>
        {isExportOpen && (
          <div className="absolute right-0 top-full mt-1 min-w-[160px] rounded-lg border border-zinc-200 bg-white py-1 shadow-lg dark:border-zinc-700 dark:bg-zinc-900">
            <div className="px-3 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">{t("imageSection")}</div>
            <button
              onClick={() => { setIsExportOpen(false); exportCanvasImage(nodes, "png", ontologyName); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportPng")}
            </button>
            <button
              onClick={() => { setIsExportOpen(false); exportCanvasImage(nodes, "svg", ontologyName); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportSvg")}
            </button>
            <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
            <div className="px-3 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">{t("schemaSection")}</div>
            <button
              onClick={async () => { setIsExportOpen(false); await onExportSchema("json"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportJson")}
            </button>
            <button
              onClick={async () => { setIsExportOpen(false); await onExportSchema("cypher"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportCypher")}
            </button>
            <button
              onClick={async () => { setIsExportOpen(false); await onExportSchema("mermaid"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportMermaid")}
            </button>
            <button
              onClick={async () => { setIsExportOpen(false); await onExportSchema("graphql"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportGraphql")}
            </button>
            <button
              onClick={async () => { setIsExportOpen(false); await onExportSchema("owl"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportOwl")}
            </button>
            <button
              onClick={async () => { setIsExportOpen(false); await onExportSchema("shacl"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
            >
              {t("exportShacl")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

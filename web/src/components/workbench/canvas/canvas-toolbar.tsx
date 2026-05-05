"use client";

import { useCallback, useRef } from "react";
import { useTranslations } from "next-intl";
import type { Node } from "@xyflow/react";

import { useClickOutside } from "@/hooks/use-click-outside";
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
  const tImage = useTranslations("workbench.canvas.toolbar.image");
  const exportRef = useRef<HTMLDivElement>(null);
  const closeExport = useCallback(() => setIsExportOpen(false), [setIsExportOpen]);
  useClickOutside(exportRef, closeExport, isExportOpen);

  const imageCopy = {
    nothingToExport: tImage("nothingToExport"),
    exported: tImage("exported"),
    failed: tImage("failed"),
  };

  return (
    <div className="absolute end-2 top-2 z-canvas flex flex-wrap items-center justify-end gap-1.5">
      <PerspectiveSwitcher
        nodes={nodes}
        topologySignature={topologySignature}
        onApplyPositions={onApplyPositions}
        onOpen={() => setIsExportOpen(false)}
      />
      <div ref={exportRef} className="relative">
        <button type="button"
          onClick={() => setIsExportOpen((v) => !v)}
          className="flex items-center rounded-md border border-divider bg-surface-base px-2 py-1 text-2xs font-medium text-foreground shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-raised"
        >
          {t("export")}
        </button>
        {isExportOpen && (
          <div className="absolute end-0 top-full mt-1 min-w-[160px] rounded-lg border border-divider bg-surface-base py-1 shadow-3">
            <div className="px-3 py-1 text-2xs font-medium uppercase tracking-wider text-foreground-muted">{t("imageSection")}</div>
            <button type="button"
              onClick={() => { setIsExportOpen(false); exportCanvasImage(nodes, "png", ontologyName, imageCopy); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportPng")}
            </button>
            <button type="button"
              onClick={() => { setIsExportOpen(false); exportCanvasImage(nodes, "svg", ontologyName, imageCopy); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportSvg")}
            </button>
            <div className="my-1 h-px bg-surface-inset" />
            <div className="px-3 py-1 text-2xs font-medium uppercase tracking-wider text-foreground-muted">{t("schemaSection")}</div>
            <button type="button"
              onClick={async () => { setIsExportOpen(false); await onExportSchema("json"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportJson")}
            </button>
            <button type="button"
              onClick={async () => { setIsExportOpen(false); await onExportSchema("cypher"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportCypher")}
            </button>
            <button type="button"
              onClick={async () => { setIsExportOpen(false); await onExportSchema("mermaid"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportMermaid")}
            </button>
            <button type="button"
              onClick={async () => { setIsExportOpen(false); await onExportSchema("graphql"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportGraphql")}
            </button>
            <button type="button"
              onClick={async () => { setIsExportOpen(false); await onExportSchema("owl"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportOwl")}
            </button>
            <button type="button"
              onClick={async () => { setIsExportOpen(false); await onExportSchema("shacl"); }}
              className="flex w-full items-center px-3 py-1.5 text-xs text-foreground hover:bg-surface-inset"
            >
              {t("exportShacl")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

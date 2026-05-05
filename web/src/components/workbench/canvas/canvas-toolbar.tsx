"use client";

import { useCallback, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import type { Node } from "@xyflow/react";

import { useClickOutside } from "@/hooks/use-click-outside";
import { exportCanvasImage } from "./canvas-helpers";
import { PerspectiveSwitcher } from "./perspective-switcher";
import { ELK_LAYOUT_PRESETS, type ElkLayoutPreset } from "./elk-layout";
import { cn } from "@/lib/cn";
import type { ExportFormat } from "@/lib/export-utils";

interface CanvasToolbarProps {
  nodes: Node[];
  ontologyName: string;
  topologySignature: string;
  isExportOpen: boolean;
  setIsExportOpen: (v: boolean | ((prev: boolean) => boolean)) => void;
  onExportSchema: (format: ExportFormat) => Promise<void>;
  onApplyPositions: (positions: Record<string, { x: number; y: number }>) => void;
  /**
   * Active ELK layout preset. The toolbar renders a picker so the
   * user can switch between hierarchical / tree / radial / force /
   * stress without leaving the canvas. Optional — surfaces that
   * pin a single layout (read-only embeds) omit both props and the
   * picker disappears.
   */
  layout?: ElkLayoutPreset;
  /** Fires when the user picks a different layout preset. */
  onLayoutChange?: (preset: ElkLayoutPreset) => void;
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
  layout,
  onLayoutChange,
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
    <div className="absolute end-2 top-2 z-canvas flex max-w-[calc(100%-1rem)] flex-nowrap items-center justify-end gap-1.5">
      <PerspectiveSwitcher
        nodes={nodes}
        topologySignature={topologySignature}
        onApplyPositions={onApplyPositions}
        onOpen={() => setIsExportOpen(false)}
      />
      {layout && onLayoutChange && (
        <LayoutPicker
          layout={layout}
          onLayoutChange={onLayoutChange}
          onOpen={() => setIsExportOpen(false)}
        />
      )}
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

// ---------------------------------------------------------------------------
// LayoutPicker — ELK preset switcher
// ---------------------------------------------------------------------------

interface LayoutPickerProps {
  layout: ElkLayoutPreset;
  onLayoutChange: (preset: ElkLayoutPreset) => void;
  /**
   * Optional close-other-popovers callback. Wired so opening this
   * picker closes the export menu sitting next to it (and vice
   * versa) — only one toolbar dropdown should sit open at a time.
   */
  onOpen?: () => void;
}

function LayoutPicker({ layout, onLayoutChange, onOpen }: LayoutPickerProps) {
  const t = useTranslations("workbench.canvas.toolbar.layout");
  const ref = useRef<HTMLDivElement>(null);
  const [isOpen, setIsOpen] = useState(false);
  useClickOutside(ref, () => setIsOpen(false), isOpen);
  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => {
          if (!isOpen) onOpen?.();
          setIsOpen((v) => !v);
        }}
        className="flex items-center gap-1 rounded-md border border-divider bg-surface-base px-2 py-1 text-2xs font-medium text-foreground shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-raised"
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-label={t("ariaLabel")}
      >
        <span className="text-foreground-muted">{t("label")}</span>
        <span className="font-semibold">{t(layout)}</span>
      </button>
      {isOpen && (
        <ul
          role="listbox"
          aria-label={t("ariaLabel")}
          className="absolute end-0 top-full mt-1 min-w-[160px] rounded-lg border border-divider bg-surface-base py-1 shadow-3"
        >
          {ELK_LAYOUT_PRESETS.map((preset) => {
            const active = preset.id === layout;
            return (
              <li key={preset.id}>
                <button
                  type="button"
                  role="option"
                  aria-selected={active}
                  onClick={() => {
                    onLayoutChange(preset.id);
                    setIsOpen(false);
                  }}
                  className={cn(
                    "flex w-full items-center justify-between px-3 py-1.5 text-xs hover:bg-surface-inset",
                    active
                      ? "font-semibold text-brand-foreground"
                      : "text-foreground",
                  )}
                >
                  <span>{t(preset.labelKey)}</span>
                  {active && (
                    <span aria-hidden className="text-brand-foreground">
                      ✓
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

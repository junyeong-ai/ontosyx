"use client";

import { memo } from "react";
import { useTranslations } from "next-intl";
import type { GraphNodeData } from "./graph-types";
import { formatValue } from "../chart-utils";
import { useLocaleChain } from "@/hooks/use-locale-chain";

// ---------------------------------------------------------------------------
// GraphDetailPanel — shows selected node properties
// ---------------------------------------------------------------------------

interface NodeDetailPanelProps {
  node: GraphNodeData;
  onClose: () => void;
}

export const GraphDetailPanel = memo(function GraphDetailPanel({
  node,
  onClose,
}: NodeDetailPanelProps) {
  const t = useTranslations("widget.graph");
  const localeChain = useLocaleChain();
  const entries = Object.entries(node.properties).filter(
    ([, v]) => v != null,
  );

  return (
    <div
      className="absolute end-2 top-2 z-canvas w-64 overflow-hidden rounded-lg border border-divider bg-surface-base shadow-3"
      role="region"
      aria-label={t("detailAria", { label: node.label })}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div className="flex items-center justify-between border-b border-divider-soft px-3 py-2">
        <div className="min-w-0">
          <div className="truncate text-xs font-semibold text-foreground-strong">
            {node.label}
          </div>
          {node.type && (
            <div className="truncate text-2xs text-foreground-muted">
              {node.type}
            </div>
          )}
        </div>
        <button
          type="button"
          onClick={onClose}
          className="ms-2 flex h-5 w-5 shrink-0 items-center justify-center rounded text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset hover:text-foreground-muted"
          aria-label={t("closeDetail")}
        >
          <svg viewBox="0 0 12 12" className="h-3 w-3" fill="currentColor" aria-hidden="true">
            <path d="M3.05 3.05a.5.5 0 01.7 0L6 5.29l2.25-2.24a.5.5 0 01.7.7L6.71 6l2.24 2.25a.5.5 0 01-.7.7L6 6.71 3.75 8.95a.5.5 0 01-.7-.7L5.29 6 3.05 3.75a.5.5 0 010-.7z" />
          </svg>
        </button>
      </div>
      <div className="max-h-64 overflow-auto px-3 py-2">
        {entries.length > 0 ? (
          <dl className="space-y-1.5">
            {entries.map(([key, val]) => (
              <div key={key} className="text-2xs">
                <dt className="font-medium text-foreground-muted">
                  {key}
                </dt>
                <dd className="mt-0.5 text-foreground break-words">
                  {formatValue(val, localeChain)}
                </dd>
              </div>
            ))}
          </dl>
        ) : (
          <p className="text-2xs text-foreground-muted">{t("noProperties")}</p>
        )}
      </div>
    </div>
  );
});

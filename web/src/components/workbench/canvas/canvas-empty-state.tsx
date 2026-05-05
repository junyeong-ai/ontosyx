"use client";

import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Heading } from "@/components/ui/heading";
import {
  Alert02Icon,
  DatabaseIcon,
  PencilEdit01Icon,
  Upload04Icon,
} from "@hugeicons/core-free-icons";

import { useAppStore } from "@/lib/store";

/**
 * Canvas placeholder rendered when no ontology is loaded.
 *
 * Shows a "Ready to Design" prompt when a project exists, or a create/import
 * pair when no project has been loaded yet.
 */
export function CanvasEmptyState() {
  const t = useTranslations("workbench.canvas.empty");
  const hasProject = useAppStore((s) => s.activeProject !== null);

  if (hasProject) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-brand-surface">
          <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-brand-foreground" size="100%" />
        </div>
        <div className="text-center">
          <Heading level={2} size={4}>{t("readyTitle")}</Heading>
          <p className="mt-1.5 max-w-md text-sm text-foreground">
            {t.rich("readyHint", {
              bold: (chunks) => <strong>{chunks}</strong>,
            })}
          </p>
        </div>
        <button type="button"
          onClick={() => {
            const s = useAppStore.getState();
            s.setDesignBottomTab("workflow");
            if (!s.isBottomPanelOpen) s.toggleBottomPanel();
          }}
          className="rounded-lg bg-brand-solid px-4 py-2 text-xs font-medium text-foreground-onbrand transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid"
        >
          {t("openWorkflow")}
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 p-8">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-brand-surface">
        <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-brand-foreground" size="100%" />
      </div>
      <div className="text-center">
        <Heading level={2} size={4}>{t("startTitle")}</Heading>
        <p className="mt-1.5 max-w-md text-sm text-foreground">
          {t("startHint")}
        </p>
      </div>
      <div className="flex items-center gap-4">
        <button type="button"
          onClick={() => {
            const s = useAppStore.getState();
            s.setDesignBottomTab("workflow");
            if (!s.isBottomPanelOpen) s.toggleBottomPanel();
          }}
          className="flex flex-col items-center gap-2 rounded-xl border border-divider bg-surface-base p-5 text-center transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] hover:border-brand-border hover:shadow-2"
        >
          <HugeiconsIcon icon={DatabaseIcon} className="h-5 w-5 text-brand-foreground" size="100%" />
          <span className="text-xs font-medium text-foreground">{t("createProject")}</span>
          <span className="text-2xs text-foreground-muted">{t("createProjectHint")}</span>
        </button>
        <span className="text-xs text-foreground-muted">{t("or")}</span>
        <button type="button"
          onClick={() => {
            const fileInput = document.querySelector('input[type="file"][accept=".json,.ttl,.owl"]') as HTMLInputElement;
            fileInput?.click();
          }}
          className="flex flex-col items-center gap-2 rounded-xl border border-divider bg-surface-base p-5 text-center transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] hover:border-brand-border hover:shadow-2"
        >
          <HugeiconsIcon icon={Upload04Icon} className="h-5 w-5 text-concept-foreground" size="100%" />
          <span className="text-xs font-medium text-foreground">{t("importOntology")}</span>
          <span className="text-2xs text-foreground-muted">{t("importOntologyHint")}</span>
        </button>
      </div>
    </div>
  );
}

/**
 * Canvas placeholder rendered when an ontology exists but contains
 * zero node types — typically the result of every source table being
 * excluded during analysis review or a sparse design pass that
 * produced no node candidates.
 *
 * The CTA jumps the operator to the analysis review's exclusions
 * section so the cause is one click away from being fixed.
 */
export function CanvasZeroNodesState() {
  const t = useTranslations("workbench.canvas.zeroNodes");

  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-warning-surface/30">
        <HugeiconsIcon
          icon={Alert02Icon}
          className="h-6 w-6 text-warning-foreground"
          size="100%"
        />
      </div>
      <div className="text-center">
        <Heading level={2} size={4}>
          {t("title")}
        </Heading>
        <p className="mt-1.5 max-w-md text-sm text-foreground">
          {t("description")}
        </p>
      </div>
      <button type="button"
        onClick={() => {
          const s = useAppStore.getState();
          s.setDesignBottomTab("workflow");
          if (!s.isBottomPanelOpen) s.toggleBottomPanel();
          // Wait for the panel to open before scrolling the anchor
          // into view; the bottom panel's content lazy-mounts.
          requestAnimationFrame(() => {
            document
              .getElementById("review-exclusions")
              ?.scrollIntoView({ behavior: "smooth", block: "start" });
          });
        }}
        className="rounded-lg bg-brand-solid px-4 py-2 text-xs font-medium text-foreground-onbrand transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid"
      >
        {t("openReview")}
      </button>
    </div>
  );
}

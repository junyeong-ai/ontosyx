"use client";

import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
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
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
          <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-emerald-500" size="100%" />
        </div>
        <div className="text-center">
          <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">{t("readyTitle")}</h2>
          <p className="mt-1.5 max-w-md text-sm text-zinc-600 dark:text-muted-foreground">
            {t.rich("readyHint", {
              bold: (chunks) => <strong>{chunks}</strong>,
            })}
          </p>
        </div>
        <button
          onClick={() => {
            const s = useAppStore.getState();
            s.setDesignBottomTab("workflow");
            if (!s.isBottomPanelOpen) s.toggleBottomPanel();
          }}
          className="rounded-lg bg-emerald-600 px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-emerald-700"
        >
          {t("openWorkflow")}
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 p-8">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
        <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-emerald-500" size="100%" />
      </div>
      <div className="text-center">
        <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">{t("startTitle")}</h2>
        <p className="mt-1.5 max-w-md text-sm text-zinc-600 dark:text-muted-foreground">
          {t("startHint")}
        </p>
      </div>
      <div className="flex items-center gap-4">
        <button
          onClick={() => {
            const s = useAppStore.getState();
            s.setDesignBottomTab("workflow");
            if (!s.isBottomPanelOpen) s.toggleBottomPanel();
          }}
          className="flex flex-col items-center gap-2 rounded-xl border border-zinc-200 bg-white p-5 text-center transition-all hover:border-emerald-300 hover:shadow-md dark:border-zinc-700 dark:bg-zinc-900 dark:hover:border-emerald-700"
        >
          <HugeiconsIcon icon={DatabaseIcon} className="h-5 w-5 text-emerald-600 dark:text-emerald-400" size="100%" />
          <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">{t("createProject")}</span>
          <span className="text-[10px] text-muted-foreground">{t("createProjectHint")}</span>
        </button>
        <span className="text-xs text-muted-foreground">{t("or")}</span>
        <button
          onClick={() => {
            const fileInput = document.querySelector('input[type="file"][accept=".json,.ttl,.owl"]') as HTMLInputElement;
            fileInput?.click();
          }}
          className="flex flex-col items-center gap-2 rounded-xl border border-zinc-200 bg-white p-5 text-center transition-all hover:border-emerald-300 hover:shadow-md dark:border-zinc-700 dark:bg-zinc-900 dark:hover:border-emerald-700"
        >
          <HugeiconsIcon icon={Upload04Icon} className="h-5 w-5 text-indigo-600 dark:text-indigo-400" size="100%" />
          <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">{t("importOntology")}</span>
          <span className="text-[10px] text-muted-foreground">{t("importOntologyHint")}</span>
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
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-amber-50 dark:bg-amber-950/30">
        <HugeiconsIcon
          icon={Alert02Icon}
          className="h-6 w-6 text-amber-500"
          size="100%"
        />
      </div>
      <div className="text-center">
        <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">
          {t("title")}
        </h2>
        <p className="mt-1.5 max-w-md text-sm text-zinc-600 dark:text-muted-foreground">
          {t("description")}
        </p>
      </div>
      <button
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
        className="rounded-lg bg-emerald-600 px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-emerald-700"
      >
        {t("openReview")}
      </button>
    </div>
  );
}

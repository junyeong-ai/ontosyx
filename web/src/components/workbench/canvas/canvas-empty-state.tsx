"use client";

import { HugeiconsIcon } from "@hugeicons/react";
import { PencilEdit01Icon, DatabaseIcon, Upload04Icon } from "@hugeicons/core-free-icons";

import { useAppStore } from "@/lib/store";

/**
 * Canvas placeholder rendered when no ontology is loaded.
 *
 * Shows a "Ready to Design" prompt when a project exists, or a create/import
 * pair when no project has been loaded yet.
 */
export function CanvasEmptyState() {
  const hasProject = useAppStore((s) => s.activeProject !== null);

  if (hasProject) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
          <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-emerald-500" size="100%" />
        </div>
        <div className="text-center">
          <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">Ready to Design</h2>
          <p className="mt-1.5 max-w-md text-sm text-zinc-600 dark:text-zinc-400">
            Review the analysis in the Workflow panel below, then click <strong>Design Ontology</strong> to generate your knowledge graph schema.
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
          Open Workflow Panel
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
        <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">Start Designing</h2>
        <p className="mt-1.5 max-w-md text-sm text-zinc-600 dark:text-zinc-400">
          Create a project from a data source or import an existing ontology to begin designing your knowledge graph.
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
          <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">Create Project</span>
          <span className="text-[10px] text-zinc-400">Database, CSV, JSON, or code repo</span>
        </button>
        <span className="text-xs text-zinc-400">or</span>
        <button
          onClick={() => {
            const fileInput = document.querySelector('input[type="file"][accept=".json,.ttl,.owl"]') as HTMLInputElement;
            fileInput?.click();
          }}
          className="flex flex-col items-center gap-2 rounded-xl border border-zinc-200 bg-white p-5 text-center transition-all hover:border-emerald-300 hover:shadow-md dark:border-zinc-700 dark:bg-zinc-900 dark:hover:border-emerald-700"
        >
          <HugeiconsIcon icon={Upload04Icon} className="h-5 w-5 text-indigo-600 dark:text-indigo-400" size="100%" />
          <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">Import Ontology</span>
          <span className="text-[10px] text-zinc-400">JSON, OWL, or Turtle file</span>
        </button>
      </div>
    </div>
  );
}

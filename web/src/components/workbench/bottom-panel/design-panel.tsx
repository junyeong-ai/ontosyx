"use client";

import { useAppStore } from "@/lib/store";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { CreateProjectForm } from "./create-project-form";
import { PhaseStepper } from "./phase-stepper";
import { ProjectWorkflow } from "./project-workflow";
import { RecentProjects } from "./recent-projects";

// ---------------------------------------------------------------------------
// Design Panel — project-based ontology design lifecycle (orchestrator)
// ---------------------------------------------------------------------------

export function DesignPanel() {
  const project = useAppStore((s) => s.activeProject);
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);
  const guardPendingEdits = useGuardPendingEdits();

  if (!project) {
    // Centred narrow column for the create form: the surrounding
    // DesignLayout promotes this panel to the main pane while no
    // ontology exists, so the canvas is gone and the form would
    // otherwise sit pinned to the left of a wide empty viewport.
    //
    // The stepper sits above the form at index `-1` so a first-time
    // operator immediately sees the full lifecycle (analyze → design
    // → complete) and understands which step they are about to enter.
    return (
      <div className="h-full overflow-auto px-4 py-6">
        <div className="mx-auto w-full max-w-3xl space-y-6">
          <PhaseStepper currentStepIndex={-1} />
          <CreateProjectForm
            guardBeforeCreate={guardPendingEdits}
            onCreated={(p) => applyProjectSnapshot(p)}
          />
          <RecentProjects />
        </div>
      </div>
    );
  }

  // Project workflow is a fixed-left + flex-right two-column layout.
  // Letting it stretch to the full pane width gives the right-hand
  // analysis review breathing room on wide screens — the left column
  // already has its own responsive ceiling
  // (`w-80 → xl:w-96 → 2xl:w-[480px]`), so the right column absorbs
  // the surplus instead of stranding it as dead space.
  return (
    <div className="h-full overflow-auto">
      <ProjectWorkflow
        project={project}
        applyProjectSnapshot={applyProjectSnapshot}
      />
    </div>
  );
}

"use client";

import { useAppStore } from "@/lib/store";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { CanonicalStatusBanner } from "./canonical-status-banner";
import { CreateOntologyDraftForm } from "./create-ontology-draft-form";
import { PhaseStepper } from "./phase-stepper";
import { OntologyDraftWorkflow } from "./ontology-draft-workflow";
import { RecentOntologyDrafts } from "./recent-ontology-drafts";

// ---------------------------------------------------------------------------
// Design Panel — project-based ontology design lifecycle (orchestrator)
// ---------------------------------------------------------------------------

export function DesignPanel() {
  const project = useAppStore((s) => s.activeOntologyDraft);
  const applyOntologyDraftSnapshot = useAppStore((s) => s.applyOntologyDraftSnapshot);
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
          <CanonicalStatusBanner />
          <PhaseStepper currentStepIndex={-1} />
          <CreateOntologyDraftForm
            guardBeforeCreate={guardPendingEdits}
            onCreated={(p) => applyOntologyDraftSnapshot(p)}
          />
          <RecentOntologyDrafts />
        </div>
      </div>
    );
  }

  // Ontology draft workflow is a fixed-left + flex-right two-column layout.
  // Letting it stretch to the full pane width gives the right-hand
  // analysis review breathing room on wide screens — the left column
  // already has its own responsive ceiling
  // (`w-80 → xl:w-96 → 2xl:w-[480px]`), so the right column absorbs
  // the surplus instead of stranding it as dead space.
  return (
    <div className="h-full overflow-auto">
      <OntologyDraftWorkflow
        project={project}
        applyOntologyDraftSnapshot={applyOntologyDraftSnapshot}
      />
    </div>
  );
}

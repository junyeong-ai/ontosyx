"use client";

import { useAppStore } from "@/lib/store";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import type { OntologyIR } from "@/types/api";
import { CreateProjectForm } from "./create-project-form";
import { ProjectWorkflow } from "./project-workflow";

// ---------------------------------------------------------------------------
// Design Panel — project-based ontology design lifecycle (orchestrator)
// ---------------------------------------------------------------------------

export function DesignPanel() {
  const project = useAppStore((s) => s.activeProject);
  const setProject = useAppStore((s) => s.setActiveProject);
  const setOntology = useAppStore((s) => s.setOntology);
  const guardPendingEdits = useGuardPendingEdits();

  if (!project) {
    return (
      <div className="h-full overflow-auto p-4">
        <CreateProjectForm
          guardBeforeCreate={guardPendingEdits}
          onCreated={(p) => {
            setProject(p);
            if (p.ontology) setOntology(p.ontology as OntologyIR);
          }}
        />
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto">
      <ProjectWorkflow
        project={project}
        setProject={setProject}
        setOntology={setOntology}
      />
    </div>
  );
}

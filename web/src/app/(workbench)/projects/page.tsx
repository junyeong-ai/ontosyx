import { ProjectHub } from "@/components/workbench/projects/project-hub";

/**
 * Project Hub — full card grid of every design project the operator
 * can see (ADR-0055). Sits next to `/design` (the active-project
 * canvas) in the workbench: design is for editing the project the
 * operator is working on right now; the hub is the place to pick
 * one up, browse history, and start a new one.
 *
 * The compact 5-row "recent" list inside the design panel stays as
 * the in-flight surface — operators picking up where they left off
 * stay in `/design`. The hub answers "what else have I worked on?"
 * without scrolling through the design panel's narrow column.
 */
export default function ProjectsPage() {
  return <ProjectHub />;
}

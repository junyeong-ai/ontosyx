import { GlossaryWorkbench } from "@/components/workbench/glossary/glossary-workbench";

/**
 * Vocabulary workbench mode (5th — alongside Design / Analyze /
 * Explore / Dashboard). Glossary terms carry the workspace's
 * canonical vocabulary plus the optional `realisation` that promotes
 * a term to a workspace-canonical business concept; every other
 * workspace mode reads from this surface.
 *
 * `?term=<id>` deep-links a specific term; the workbench falls back
 * to the first term if the id is unknown.
 */
export default function GlossaryPage() {
  return <GlossaryWorkbench />;
}

import { GlossaryWorkbench } from "@/components/workbench/glossary/glossary-workbench";

/**
 * Vocabulary workbench mode (5th — alongside Design / Analyze /
 * Explore / Dashboard). Promoted out of `/settings/glossary` because
 * the Concept layer (ConceptDef + GlossaryTermDef + TermRealisation,
 * see ADR-0014) is cross-cutting: every other workspace mode reads
 * it. ADR-0058.
 *
 * `?term=<id>` deep-links a specific term; the workbench falls back
 * to the first term if the id is unknown.
 */
export default function GlossaryPage() {
  return <GlossaryWorkbench />;
}

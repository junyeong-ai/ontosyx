import { VocabularyWorkbench } from "@/components/workbench/vocabulary/vocabulary-workbench";

/**
 * Vocabulary workbench mode (sixth — alongside Design / Analyze /
 * Explore / Dashboard / Glossary). Hosts the per-workspace
 * vocabulary registries (code systems, value sets, concept maps,
 * notation patterns) that previously sat under /settings/* as
 * disconnected editorial pages. Designers edit them next to the
 * glossary instead of context-switching to the settings sidebar.
 *
 * `?tab=<id>` deep-links a specific registry; defaults to
 * code-systems.
 */
export default function VocabularyPage() {
  return <VocabularyWorkbench />;
}

import { BookOpen, Database, Pencil, Search } from "lucide-react";
import { Code2, Info, Network, TrendingUp } from "lucide-react";
// ---------------------------------------------------------------------------
// Tool metadata for rich display
// ---------------------------------------------------------------------------

export const TOOL_META: Record<string, { label: string; icon: typeof Database; verb: string }> = {
  query_graph: { label: "Graph Query", icon: Database, verb: "Querying graph" },
  edit_ontology: { label: "Edit Ontology", icon: Pencil, verb: "Editing ontology" },
  apply_ontology: { label: "Apply Edit", icon: Pencil, verb: "Applying changes" },
  execute_analysis: { label: "Analysis", icon: Code2, verb: "Running analysis" },
  explain_ontology: { label: "Explain", icon: Info, verb: "Explaining ontology" },
  visualize: { label: "Visualize", icon: TrendingUp, verb: "Generating chart" },
  recall_memory: { label: "Memory", icon: Search, verb: "Searching memory" },
  search_recipes: { label: "Recipes", icon: Search, verb: "Searching recipes" },
  introspect_source: { label: "Schema Explorer", icon: Database, verb: "Exploring schema" },
  schema_evolution: { label: "Schema Evolution", icon: Database, verb: "Analyzing drift" },
  consult_knowledge: { label: "Knowledge", icon: BookOpen, verb: "Searching knowledge" },
  raw_cypher: { label: "Raw Cypher", icon: Database, verb: "Executing query" },
};

// ---------------------------------------------------------------------------
// Sub-step labels for tool progress display (machine-readable → Korean)
// ---------------------------------------------------------------------------

/** Full step labels for inline progress (chat panel). */
export const STEP_LABELS: Record<string, string> = {
  schema_discovery: "Schema Discovery",
  knowledge_lookup: "Knowledge Lookup",
  llm_primary: "AI Translation",
  llm_fallback: "AI Translation (retry)",
  llm_retry: "AI Translation (final)",
  compiling: "Compiling",
  executing: "Executing",
};

/** Short step labels for timing badges (results panel). */
export const STEP_TIMING_LABELS: Record<string, string> = {
  translating: "Translate",
  schema_discovery: "Schema",
  knowledge_lookup: "Knowledge",
  llm_primary: "AI",
  llm_fallback: "Retry",
  llm_retry: "Final",
  compiling: "Compile",
  executing: "Execute",
};
export const DEFAULT_TOOL_META = { label: "Tool", icon: Network, verb: "Processing" };

import {
  DatabaseIcon,
  PencilEdit01Icon,
  SourceCodeIcon,
  InformationCircleIcon,
  ChartLineData02Icon,
  Search01Icon,
  AiNetworkIcon,
  BookOpen01Icon,
} from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// Tool metadata for rich display
// ---------------------------------------------------------------------------

export const TOOL_META: Record<string, { label: string; icon: typeof DatabaseIcon; verb: string }> = {
  query_graph: { label: "Graph Query", icon: DatabaseIcon, verb: "Querying graph" },
  edit_ontology: { label: "Edit Ontology", icon: PencilEdit01Icon, verb: "Editing ontology" },
  apply_ontology: { label: "Apply Edit", icon: PencilEdit01Icon, verb: "Applying changes" },
  execute_analysis: { label: "Analysis", icon: SourceCodeIcon, verb: "Running analysis" },
  explain_ontology: { label: "Explain", icon: InformationCircleIcon, verb: "Explaining ontology" },
  visualize: { label: "Visualize", icon: ChartLineData02Icon, verb: "Generating chart" },
  recall_memory: { label: "Memory", icon: Search01Icon, verb: "Searching memory" },
  search_recipes: { label: "Recipes", icon: Search01Icon, verb: "Searching recipes" },
  introspect_source: { label: "Schema Explorer", icon: DatabaseIcon, verb: "Exploring schema" },
  schema_evolution: { label: "Schema Evolution", icon: DatabaseIcon, verb: "Analyzing drift" },
  consult_knowledge: { label: "Knowledge", icon: BookOpen01Icon, verb: "Searching knowledge" },
  raw_cypher: { label: "Raw Cypher", icon: DatabaseIcon, verb: "Executing query" },
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
export const DEFAULT_TOOL_META = { label: "Tool", icon: AiNetworkIcon, verb: "Processing" };

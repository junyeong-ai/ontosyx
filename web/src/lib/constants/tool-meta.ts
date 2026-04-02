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

export const DEFAULT_TOOL_META = { label: "Tool", icon: AiNetworkIcon, verb: "Processing" };

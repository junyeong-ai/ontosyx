// ---------------------------------------------------------------------------
// Core ontology types — IR, query, widget, commands
// ---------------------------------------------------------------------------

export type Cardinality = "one_to_one" | "one_to_many" | "many_to_one" | "many_to_many";

// Cursor-based pagination
export interface CursorPage<T> {
  items: T[];
  next_cursor?: string;
}

export interface OntologyIR {
  id: string;
  name: string;
  description?: string | null;
  version: number;
  node_types: NodeTypeDef[];
  edge_types: EdgeTypeDef[];
  indexes?: IndexDef[];
}

export interface NodeTypeDef {
  id: string;
  label: string;
  description?: string | null;
  /** Source table name this node was derived from (set by LLM for DB sources) */
  source_table?: string | null;
  properties: PropertyDef[];
  constraints?: ConstraintDef[];
}

export interface EdgeTypeDef {
  id: string;
  label: string;
  description?: string | null;
  source_node_id: string;
  target_node_id: string;
  properties: PropertyDef[];
  cardinality?: Cardinality;
}

/** Tagged property type from backend: `{"type": "string"}`, `{"type": "list", "element": {...}}` */
export type PropertyType = { type: string; element?: PropertyType };

/** Display a PropertyType as a human-readable string, e.g. "string", "list<int>" */
export function formatPropertyType(pt: PropertyType): string {
  if (pt.type === "list" && pt.element) {
    return `list<${formatPropertyType(pt.element)}>`;
  }
  return pt.type;
}

export interface PropertyDef {
  id: string;
  name: string;
  property_type: PropertyType;
  nullable?: boolean;
  default_value?: unknown;
  description?: string | null;
  /** Source column name this property was derived from (set by LLM for DB sources) */
  source_column?: string | null;
}

export type ConstraintDef =
  | { id: string; type: "unique"; property_ids: string[] }
  | { id: string; type: "exists"; property_id: string }
  | { id: string; type: "node_key"; property_ids: string[] };

export interface IndexDef {
  id: string;
  type: string;
  node_id: string;
  property_id?: string;
  property_ids?: string[];
  name?: string;
  dimensions?: number;
  similarity?: string;
}

export interface QueryIR {
  operation: QueryOp;
  limit?: number | null;
  skip?: number | null;
  order_by: OrderClause[];
}

export type QueryOp = Record<string, unknown> & {
  op: string;
};

export interface OrderClause {
  projection: Record<string, unknown>;
  direction: "asc" | "desc";
}

export interface QueryResult {
  columns: string[];
  rows: Record<string, unknown>[];
  metadata?: Record<string, unknown>;
}

export type WidgetSpec = Record<string, unknown> & {
  widget_type?: string;
  title?: string;
  reason?: string;
  chart_type?: string;
  content?: string;
  x_axis?: { field?: string };
  y_axis?: { field?: string };
  data_mapping?: {
    label?: string;
    value?: string;
    delta?: string;
  };
  series?: Array<{ field?: string }>;
  columns?: Array<{ key: string; label?: string }>;
  thresholds?: {
    warning?: number;
    critical?: number;
    direction?: "above" | "below";
  };
  // Graph-specific fields (from GraphSpec)
  node_config?: NodeVizConfig;
  edge_config?: EdgeVizConfig;
  layout?: GraphLayout;
  interactive?: boolean;
  zoom_enabled?: boolean;
  max_nodes?: number;
};

// ---------------------------------------------------------------------------
// GraphSpec — interactive graph visualization types
// ---------------------------------------------------------------------------

export type GraphLayout = "force" | "hierarchical" | "radial" | "dagre";

export interface NodeVizConfig {
  label_field: string;
  color_field?: string;
  color_map?: ColorMapping[];
  size_field?: string;
  tooltip_fields: string[];
}

export interface EdgeVizConfig {
  label_field?: string;
  color_field?: string;
  weight_field?: string;
  directed: boolean;
}

export interface ColorMapping {
  value: string;
  color: string;
}

// --- OntologyCommand (command engine for graph editing) ---

export type OntologyCommand =
  | { op: "add_node"; id: string; label: string; description?: string; source_table?: string }
  | { op: "delete_node"; node_id: string }
  | { op: "rename_node"; node_id: string; new_label: string }
  | { op: "update_node_description"; node_id: string; description?: string }
  | { op: "add_edge"; id: string; label: string; source_node_id: string; target_node_id: string; cardinality: Cardinality }
  | { op: "delete_edge"; edge_id: string }
  | { op: "rename_edge"; edge_id: string; new_label: string }
  | { op: "update_edge_cardinality"; edge_id: string; cardinality: Cardinality }
  | { op: "update_edge_description"; edge_id: string; description?: string }
  | { op: "add_property"; owner_id: string; property: PropertyDef }
  | { op: "delete_property"; owner_id: string; property_id: string }
  | { op: "update_property"; owner_id: string; property_id: string; patch: PropertyPatch }
  | { op: "add_constraint"; node_id: string; constraint: ConstraintDef }
  | { op: "remove_constraint"; node_id: string; constraint_id: string }
  | { op: "add_index"; index: IndexDef }
  | { op: "remove_index"; index_id: string }
  | { op: "batch"; description: string; commands: OntologyCommand[] };

export interface PropertyPatch {
  name?: string;
  property_type?: PropertyType;
  nullable?: boolean;
  default_value?: unknown | null;
  description?: string | null;
  source_column?: string | null;
}

// --- Saved Ontologies ---

export interface SavedOntology {
  id: string;
  name: string;
  description: string | null;
  version: number;
  ontology_ir: OntologyIR;
  created_by: string;
  created_at: string;
}

export interface PromptInfo {
  name: string;
  version: string;
}

// --- Element Verification ---

export interface ElementVerification {
  id: string;
  ontology_id: string;
  element_id: string;
  element_kind: "node" | "edge" | "property";
  verified_by: string;
  verified_by_name?: string;
  review_notes?: string;
  invalidated_at?: string;
  invalidation_reason?: string;
  created_at: string;
}

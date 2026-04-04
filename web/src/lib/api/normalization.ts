import type { QueryResult } from "@/types/api";

// ---------------------------------------------------------------------------
// Type guards
// ---------------------------------------------------------------------------

export function isPendingReconcile(data: unknown): data is import("@/types/api").PendingReconcile {
  return (
    !!data &&
    typeof data === "object" &&
    "report" in data &&
    "reconciled_ontology" in data
  );
}

// ---------------------------------------------------------------------------
// PropertyValue unwrapping
// ---------------------------------------------------------------------------

/**
 * Recursively unwrap PropertyValue tagged objects ({type, value}).
 * - Scalar: {type: "int", value: 42} -> 42
 * - Null:   {type: "null"} -> null
 * - List:   {type: "list", value: [...]} -> unwrapped array
 * - Map:    {type: "map", value: {k: pv, ...}} -> unwrapped object
 */
export function unwrapPropertyValue(cell: unknown): unknown {
  if (cell == null) return null;
  if (typeof cell !== "object") return cell;
  if (Array.isArray(cell)) return cell.map(unwrapPropertyValue);

  const obj = cell as Record<string, unknown>;

  // Neo4j Node/Relationship: extract user-visible properties first
  const extracted = extractNodeProperties(obj);
  if (extracted) return extracted;

  // Tagged PropertyValue: {type: "int", value: 42}
  if (!("type" in obj)) return cell;

  const t = obj.type as string;
  if (t === "null") return null;
  if (!("value" in obj)) return cell;

  const v = obj.value;
  if (t === "list" && Array.isArray(v)) return v.map(unwrapPropertyValue);
  if (t === "map" && v && typeof v === "object" && !Array.isArray(v)) {
    const out: Record<string, unknown> = {};
    for (const [k, pv] of Object.entries(v as Record<string, unknown>)) {
      out[k] = unwrapPropertyValue(pv);
    }
    return out;
  }
  return v;
}

// ---------------------------------------------------------------------------
// Node property extraction
// ---------------------------------------------------------------------------

/**
 * Detect a Neo4j node/relationship object and extract user-visible properties.
 * neo4rs serializes nodes as objects with internal metadata (labels, id, keys, etc.)
 * alongside the actual property fields, or in some versions as a structured object
 * with a `properties` sub-object.
 *
 * Returns the extracted properties object, or null if not a node/relationship.
 */
/** Internal fields injected by Neo4j driver or RLS isolation — never shown to users. */
const INTERNAL_FIELDS = new Set([
  "labels", "id", "element_id", "keys",
  "_workspace_id",          // RLS workspace isolation property
  "start_node_id", "end_node_id", "type",  // Relationship metadata
]);

/** Check if a field key is internal (should be stripped from display). */
function isInternalField(key: string): boolean {
  return INTERNAL_FIELDS.has(key);
}

export function extractNodeProperties(obj: Record<string, unknown>): Record<string, unknown> | null {
  // Structured node: { labels: [...], properties: { name, price, ... }, id?, keys? }
  if ("properties" in obj && typeof obj.properties === "object" && obj.properties !== null && !Array.isArray(obj.properties)) {
    const raw = obj.properties as Record<string, unknown>;
    const filtered: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(raw)) {
      if (!isInternalField(k)) filtered[k] = v;
    }
    return Object.keys(filtered).length > 0 ? filtered : null;
  }
  // Flat node from neo4rs: has `labels` (node) or `type` + `start_node_id`/`end_node_id` (relationship)
  if ("labels" in obj && Array.isArray(obj.labels)) {
    const props: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(obj)) {
      if (isInternalField(k)) continue;
      props[k] = v;
    }
    return Object.keys(props).length > 0 ? props : null;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Query result normalization
// ---------------------------------------------------------------------------

/**
 * Backend sends rows as Vec<Vec<PropertyValue>> where PropertyValue = {type, value}.
 * Frontend expects rows as Record<string, unknown>[].
 * This function normalizes the result, unwrapping PropertyValue wrappers
 * and flattening single-column node results into multi-column rows.
 */
export function normalizeQueryResult(raw: unknown): QueryResult | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const r = raw as { columns?: string[]; rows?: unknown[]; metadata?: Record<string, unknown> };
  if (!r.columns || !r.rows) return undefined;

  let columns = r.columns;
  let rows: Record<string, unknown>[] = r.rows.map((row) => {
    const obj: Record<string, unknown> = {};
    if (Array.isArray(row)) {
      // Backend format: [[{type, value}, ...], ...]
      columns.forEach((col, i) => {
        obj[col] = unwrapPropertyValue(row[i]);
      });
    } else if (row && typeof row === "object") {
      // Already object format — still unwrap nested PropertyValues
      for (const [k, v] of Object.entries(row as Record<string, unknown>)) {
        obj[k] = unwrapPropertyValue(v);
      }
    }
    return obj;
  });

  // Flatten single-column node results: if every row's single value is an object,
  // expand its keys as columns (e.g. RETURN p -> {name, price, ...} per row).
  // Also handles structured Neo4j node objects by extracting properties.
  if (columns.length === 1 && rows.length > 0) {
    const col = columns[0];
    const allObjects = rows.every(
      (row) => row[col] != null && typeof row[col] === "object" && !Array.isArray(row[col]),
    );
    if (allObjects) {
      // Try extracting node properties from structured node objects
      const extracted = rows.map((row) => {
        const val = row[col] as Record<string, unknown>;
        return extractNodeProperties(val) ?? val;
      });

      const expandedCols = new Set<string>();
      for (const obj of extracted) {
        for (const k of Object.keys(obj)) {
          expandedCols.add(k);
        }
      }
      columns = Array.from(expandedCols);
      rows = extracted;
    }
  }

  return { columns, rows, metadata: r.metadata };
}

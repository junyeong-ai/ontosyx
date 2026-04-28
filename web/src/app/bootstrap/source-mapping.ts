import type { ProjectSource } from "@/types/projects";

/**
 * Translate the bootstrap wizard's two-field source description
 * (`sourceKind` + `sourceConnection`) into the canonical
 * `ProjectSource` wire shape consumed by every post-bootstrap API
 * (preview, create, extend).
 *
 * Returns `null` when the pair can't be materialised — empty
 * connection, unknown kind, or a kind that needs structured input
 * the wizard hasn't asked for (CodeRepository, Snowflake, etc.).
 * Callers fall back to "all" semantics in that case.
 */
export function bootstrapSourceToProjectSource(
  sourceKind: string,
  sourceConnection: string,
): ProjectSource | null {
  const conn = sourceConnection.trim();
  if (!conn) return null;
  switch (sourceKind) {
    case "postgresql":
      return { type: "postgresql", connection_string: conn };
    case "mysql":
      return { type: "mysql", connection_string: conn, schema: "public" };
    case "bigquery": {
      // Wizard accepts `project_id/dataset` shorthand in the single
      // connection field — split here so the API gets typed inputs.
      const [project_id, dataset] = conn.split("/");
      if (!project_id || !dataset) return null;
      return { type: "bigquery", project_id, dataset };
    }
    case "csv":
      return { type: "csv", data: conn };
    case "json":
      return { type: "json", data: conn };
    default:
      return null;
  }
}

import type {
  CursorPage,
  ElementVerification,
  InsightSuggestion,
  OntologyDetail,
  OntologyIR,
  OntologyListItem,
} from "@/types/api";
import { request, requestText } from "./client";
import {
  CursorPageSchema,
  OntologyDetailSchema,
  OntologyIRSchema,
  OntologyListItemSchema,
} from "@/lib/validation";

// ---------------------------------------------------------------------------
// Ontologies (Λ storage model)
//
// The list endpoint returns identity rows + current-version summaries.
// The IR itself lives behind the detail endpoint — a single list page
// could otherwise pull 50 full hydrated ontologies.
// ---------------------------------------------------------------------------

export async function listOntologies(params?: {
  cursor?: string;
  limit?: number;
}): Promise<CursorPage<OntologyListItem>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  const data = await request(`/ontologies${query ? `?${query}` : ""}`);
  const result = CursorPageSchema(OntologyListItemSchema).safeParse(data);
  if (!result.success) {
    console.warn("Ontology list validation failed:", result.error.issues);
    return data as ReturnType<
      typeof CursorPageSchema<typeof OntologyListItemSchema>
    >["_output"];
  }
  return result.data;
}

/**
 * Fetch a single ontology's identity row + current-version summary +
 * fully hydrated `OntologyIR`. `ontology_ir` is `undefined` iff the
 * identity has no committed version yet (rare transitional state).
 */
export async function getOntologyDetail(id: string): Promise<OntologyDetail> {
  const data = await request(`/ontologies/${encodeURIComponent(id)}`);
  const result = OntologyDetailSchema.safeParse(data);
  if (!result.success) {
    console.warn("Ontology detail validation failed:", result.error.issues);
    return data as OntologyDetail;
  }
  return result.data as OntologyDetail;
}

// ---------------------------------------------------------------------------
// Ontology Import/Export
// ---------------------------------------------------------------------------

export async function normalizeOntology(
  input: Record<string, unknown>,
): Promise<{ ontology: OntologyIR; warnings: { kind: string; message: string }[] }> {
  const data = await request("/ontology/normalize", {
    method: "POST",
    body: JSON.stringify(input),
  }) as { ontology: unknown; warnings?: unknown[] };
  const result = OntologyIRSchema.safeParse(data.ontology);
  if (!result.success) {
    console.warn("OntologyIR validation failed:", result.error.issues);
    return { ontology: data.ontology as OntologyIR, warnings: (data.warnings ?? []) as { kind: string; message: string }[] };
  }
  return { ontology: result.data as OntologyIR, warnings: (data.warnings ?? []) as { kind: string; message: string }[] };
}

export async function exportOntology(
  ontology: OntologyIR,
): Promise<Record<string, unknown>> {
  return request("/ontology/export", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportCypher(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/cypher", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportMermaid(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/mermaid", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportGraphql(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/graphql", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportOwl(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/owl", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportShacl(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/shacl", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportTypescript(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/typescript", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportPython(ontology: OntologyIR): Promise<string> {
  return requestText("/ontology/export/python", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function importOwl(content: string): Promise<OntologyIR> {
  return request("/ontology/import/owl", {
    method: "POST",
    body: JSON.stringify({ content }),
  });
}

// ---------------------------------------------------------------------------
// Insight Suggestions
// ---------------------------------------------------------------------------

export async function suggestInsights(
  ontology: OntologyIR,
): Promise<InsightSuggestion[]> {
  return request("/ontology/suggestions", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

// ---------------------------------------------------------------------------
// Element Verification
// ---------------------------------------------------------------------------

export async function listVerifications(
  ontologyId: string,
): Promise<ElementVerification[]> {
  return request(`/ontology/${encodeURIComponent(ontologyId)}/verifications`);
}

export async function verifyElement(
  ontologyId: string,
  req: { element_id: string; element_kind: "node" | "edge" | "property"; review_notes?: string },
): Promise<{ id: string }> {
  return request(`/ontology/${encodeURIComponent(ontologyId)}/verifications`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function revokeVerification(
  ontologyId: string,
  elementId: string,
): Promise<void> {
  await request(
    `/ontology/${encodeURIComponent(ontologyId)}/verifications/${encodeURIComponent(elementId)}`,
    { method: "DELETE" },
  );
}

// ---------------------------------------------------------------------------
// Graph Audit & Adopt
// ---------------------------------------------------------------------------

export interface GraphAuditReport {
  matched_nodes: string[];
  orphan_graph_nodes: string[];
  missing_graph_nodes: string[];
  matched_edges: string[];
  orphan_graph_edges: string[];
  missing_graph_edges: string[];
  sync_status: "synced" | "partial" | "unsynced";
  sync_percentage: number;
}

export async function auditGraph(
  ontologyId: string,
): Promise<GraphAuditReport> {
  return request(`/ontology/${encodeURIComponent(ontologyId)}/audit`, {
    method: "POST",
  });
}

export async function adoptGraph(
  name?: string,
  save?: boolean,
): Promise<import("@/types/ontology").OntologyIR> {
  return request("/ontology/adopt-graph", {
    method: "POST",
    body: JSON.stringify({ name, save }),
  });
}

export async function reindexSchema(
  ontologyId: string,
): Promise<{ ontology_lineage_id: string; nodes_indexed: number }> {
  return request(`/ontology/${encodeURIComponent(ontologyId)}/reindex`, {
    method: "POST",
  });
}

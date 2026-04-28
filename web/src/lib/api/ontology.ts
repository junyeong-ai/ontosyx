import type {
  CursorPage,
  ElementVerification,
  InsightHint,
  OntologyDetail,
  OntologyIR,
  OntologyListItem,
} from "@/types/api";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";
import type { BindingEditOp } from "@/lib/api/binding-suggestions";
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
  /**
   * Exact workspace-scoped name match. When set, the server ignores
   * `cursor`/`limit` and returns a 0- or 1-element `items` array with
   * no `next_cursor`. Whitespace-only values are treated as unset.
   */
  nameEq?: string;
}): Promise<CursorPage<OntologyListItem>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  if (params?.nameEq && params.nameEq.trim())
    qs.set("name_eq", params.nameEq);
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
 * Workspace-scoped single-name lookup — returns the ontology whose
 * name matches exactly, or `null` when nothing matches. Used by the
 * Bootstrap wizard's Step 6 to detect re-entry before the
 * ontology-create POST returns 409.
 *
 * A blank/whitespace `name` short-circuits to `null` without a
 * request, matching the backend's normalisation.
 */
export async function findOntologyByName(
  name: string,
): Promise<OntologyListItem | null> {
  if (!name.trim()) return null;
  const page = await listOntologies({ nameEq: name });
  return page.items[0] ?? null;
}

// ---------------------------------------------------------------------------
// Ontology creation (unified `POST /api/ontologies`)
//
// One endpoint covers every creation shape. `initial_operations` is a
// batch of `OntologyEditOp`s applied atomically as v1 — the same op
// vocabulary used by `POST /api/ontologies/{id}/edits`, so bootstrap
// flows don't grow a parallel governance surface.
// ---------------------------------------------------------------------------

/**
 * Frontend subset of the Rust `OntologyEditOp` enum — covers only
 * the variants the current UI can construct (property bindings,
 * type deprecations, glossary-term creation). The Rust enum has
 * 30+ variants (code systems, value sets, rules, …); a TS caller
 * that needs one of those must first add its shape here.
 *
 * Naming note: we intentionally keep the `OntologyEditOp` name so
 * a caller can see at the call site that it mirrors the backend
 * vocabulary. The JSDoc above is the contract for "subset only" —
 * the union is closed, so a typo like `{ op: "create_rule" }` will
 * fail TS checks at compile time rather than at the server's
 * serde layer.
 *
 * `GlossaryTermDef` re-exports the canonical OpenAPI-generated
 * shape from `edit-ops.ts` so bootstrap and admin paths share one
 * wire contract — the Rust `OntologyEditOp::CreateGlossaryTerm`
 * deserialises the same `LocalizedText`-keyed payload either way.
 */
export type OntologyEditOp =
  | BindingEditOp
  | { op: "create_glossary_term"; def: GlossaryTermDef };

/** Request body for `POST /api/ontologies`. */
export interface CreateOntologyRequest {
  name: string;
  description?: string;
  lineage_id?: string;
  initial_operations: OntologyEditOp[];
  message?: string;
}

/** Response body returned by `POST /api/ontologies`. */
export interface CreateOntologyResponse {
  ontology_id: string;
  version_id: string;
  version: number;
  applied_operations: number;
  committed_at: string;
}

/**
 * Create a fresh ontology. Validates + routes the initial batch
 * through the same pipeline as `/edits`, so a designer-level caller
 * can submit routine CreateGlossaryTerm ops without queueing.
 */
export async function createOntology(
  body: CreateOntologyRequest,
): Promise<CreateOntologyResponse> {
  return request<CreateOntologyResponse>("/ontologies", {
    method: "POST",
    body: JSON.stringify(body),
  });
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
  const data = await request("/ontologies/normalize", {
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
  return request("/ontologies/export", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportCypher(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/cypher", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportMermaid(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/mermaid", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportGraphql(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/graphql", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportOwl(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/owl", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportShacl(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/shacl", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportTypescript(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/typescript", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function exportPython(ontology: OntologyIR): Promise<string> {
  return requestText("/ontologies/export/python", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

export async function importOwl(content: string): Promise<OntologyIR> {
  return request("/ontologies/import/owl", {
    method: "POST",
    body: JSON.stringify({ content }),
  });
}

// ---------------------------------------------------------------------------
// Insight Suggestions
// ---------------------------------------------------------------------------

export async function suggestInsights(
  ontology: OntologyIR,
): Promise<InsightHint[]> {
  return request("/ontologies/suggestions", {
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
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/verifications`);
}

export async function verifyElement(
  ontologyId: string,
  req: { element_id: string; element_kind: "node" | "edge" | "property"; review_notes?: string },
): Promise<{ id: string }> {
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/verifications`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function revokeVerification(
  ontologyId: string,
  elementId: string,
): Promise<void> {
  await request(
    `/ontologies/${encodeURIComponent(ontologyId)}/verifications/${encodeURIComponent(elementId)}`,
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
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/audit`, {
    method: "POST",
  });
}

export async function adoptGraph(
  name?: string,
  save?: boolean,
): Promise<import("@/types/ontology").OntologyIR> {
  return request("/ontologies/adopt-graph", {
    method: "POST",
    body: JSON.stringify({ name, save }),
  });
}

export async function reindexSchema(
  ontologyId: string,
): Promise<{ ontology_lineage_id: string; nodes_indexed: number }> {
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/reindex`, {
    method: "POST",
  });
}

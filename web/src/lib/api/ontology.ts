import type {
  ElementVerification,
  InsightHint,
  OntologyDetail,
  OntologyIR,
} from "@/types/api";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";
import type { BindingEditOp } from "@/lib/api/binding-suggestions";
import { request, requestText } from "./client";
import {
  OntologyDetailSchema,
  OntologyIRSchema,
} from "@/lib/validation";

// ---------------------------------------------------------------------------
// Workspace ontology — singleton (workspace × ontology = 1:1).
//
// `GET /api/ontology` returns the workspace's canonical ontology
// identity row + current-version summary + fully hydrated
// `OntologyIR`. There is no list (the workspace owns exactly one)
// and no `{id}` lookup.
// ---------------------------------------------------------------------------

/**
 * Fetch the workspace's canonical ontology — identity, current
 * version summary, and the fully hydrated IR. Returns `null` when
 * the workspace has no canonical yet (greenfield state).
 */
export async function getWorkspaceOntology(): Promise<OntologyDetail | null> {
  const { ontology } = await request<{ ontology: OntologyDetail | null }>(
    "/ontology",
  );
  if (ontology === null) return null;
  const result = OntologyDetailSchema.safeParse(ontology);
  if (!result.success) {
    console.warn("Ontology detail validation failed:", result.error.issues);
    return ontology as OntologyDetail;
  }
  return result.data as OntologyDetail;
}

// ---------------------------------------------------------------------------
// Ontology creation (`POST /api/ontology`)
//
// One endpoint creates the workspace's canonical ontology.
// `initial_operations` is a batch of `OntologyEditOp`s applied
// atomically as v1 — same op vocabulary used by `POST /api/ontology/edits`.
// 409 when the workspace already has a canonical.
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
 * wire contract.
 */
export type OntologyEditOp =
  | BindingEditOp
  | { op: "create_glossary_term"; def: GlossaryTermDef };

/** Request body for `POST /api/ontology`. */
export interface CreateOntologyRequest {
  name: string;
  description?: string;
  lineage_id?: string;
  initial_operations: OntologyEditOp[];
  message?: string;
}

/** Response body returned by `POST /api/ontology`. */
export interface CreateOntologyResponse {
  ontology_id: string;
  version_id: string;
  version: number;
  applied_operations: number;
  committed_at: string;
}

/**
 * Create the workspace's canonical ontology. Validates + routes
 * the initial batch through the same pipeline as `/edits`, so a
 * designer-level caller can submit routine CreateGlossaryTerm ops
 * without queueing.
 */
export async function createOntology(
  body: CreateOntologyRequest,
): Promise<CreateOntologyResponse> {
  return request<CreateOntologyResponse>("/ontology", {
    method: "POST",
    body: JSON.stringify(body),
  });
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
): Promise<InsightHint[]> {
  return request("/ontology/suggestions", {
    method: "POST",
    body: JSON.stringify(ontology),
  });
}

// ---------------------------------------------------------------------------
// Element Verification — singleton, no ontology id in path
// ---------------------------------------------------------------------------

export async function listVerifications(): Promise<ElementVerification[]> {
  return request("/ontology/verifications");
}

export async function verifyElement(
  req: { element_id: string; element_kind: "node" | "edge" | "property"; review_notes?: string },
): Promise<{ id: string }> {
  return request("/ontology/verifications", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function revokeVerification(elementId: string): Promise<void> {
  await request(`/ontology/verifications/${encodeURIComponent(elementId)}`, {
    method: "DELETE",
  });
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

export async function auditGraph(): Promise<GraphAuditReport> {
  return request("/ontology/audit", { method: "POST" });
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

export async function reindexSchema(): Promise<{
  ontology_id: string;
  nodes_indexed: number;
}> {
  return request("/ontology/reindex", { method: "POST" });
}

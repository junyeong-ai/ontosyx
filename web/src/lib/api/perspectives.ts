import type {
  OntologyDiff,
  RestoreProjectRevisionResponse,
  RevisionSummary,
  UpsertPerspectiveRequest,
  WorkbenchPerspective,
} from "@/types/api";
import { request } from "./client";

// ---------------------------------------------------------------------------
// Perspectives
// ---------------------------------------------------------------------------

export async function savePerspective(
  req: UpsertPerspectiveRequest,
): Promise<WorkbenchPerspective> {
  return request("/perspectives", {
    method: "PUT",
    body: JSON.stringify(req),
  });
}

export async function listPerspectives(
  lineageId: string,
): Promise<WorkbenchPerspective[]> {
  return request(`/perspectives/by-lineage/${encodeURIComponent(lineageId)}`);
}

export async function findBestPerspective(
  lineageId: string,
  topologySignature: string,
): Promise<WorkbenchPerspective | null> {
  return request(
    `/perspectives/by-lineage/${encodeURIComponent(lineageId)}/best?topology_signature=${encodeURIComponent(topologySignature)}`,
  );
}

export async function deletePerspective(id: string): Promise<void> {
  return request(`/perspectives/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Ontology Revision History
// ---------------------------------------------------------------------------

export async function listRevisions(
  ontologyDraftId: string,
): Promise<RevisionSummary[]> {
  return request(
    `/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/revisions`,
  );
}

export async function restoreRevision(
  ontologyDraftId: string,
  revision: number,
): Promise<RestoreProjectRevisionResponse> {
  return request(
    `/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/revisions/${revision}/restore`,
    { method: "POST" },
  );
}

// ---------------------------------------------------------------------------
// Ontology Revision Diff
// ---------------------------------------------------------------------------

export async function getRevisionDiff(
  ontologyDraftId: string,
  rev1: number,
  rev2: number,
): Promise<OntologyDiff> {
  return request(
    `/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/revisions/${rev1}/diff/${rev2}`,
  );
}

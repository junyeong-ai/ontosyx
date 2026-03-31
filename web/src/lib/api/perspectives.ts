import type {
  OntologyDiff,
  ProjectRestoreResponse,
  RevisionSummary,
  PerspectiveUpsertRequest,
  WorkbenchPerspective,
} from "@/types/api";
import { request } from "./client";

// ---------------------------------------------------------------------------
// Perspectives
// ---------------------------------------------------------------------------

export async function savePerspective(
  req: PerspectiveUpsertRequest,
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
  projectId: string,
): Promise<RevisionSummary[]> {
  return request(
    `/projects/${encodeURIComponent(projectId)}/revisions`,
  );
}

export async function restoreRevision(
  projectId: string,
  revision: number,
): Promise<ProjectRestoreResponse> {
  return request(
    `/projects/${encodeURIComponent(projectId)}/revisions/${revision}/restore`,
    { method: "POST" },
  );
}

// ---------------------------------------------------------------------------
// Ontology Revision Diff
// ---------------------------------------------------------------------------

export async function getRevisionDiff(
  projectId: string,
  rev1: number,
  rev2: number,
): Promise<OntologyDiff> {
  return request(
    `/projects/${encodeURIComponent(projectId)}/revisions/${rev1}/diff/${rev2}`,
  );
}

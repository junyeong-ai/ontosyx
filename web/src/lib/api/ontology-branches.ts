import { request } from "./client";
import type {
  OntologyDiffSummary,
  OntologyVersionsResponse,
  RebaseDraftResponse,
} from "@/types/ontology-branches";

/**
 * Workspace canonical version history. Newest first. Empty when
 * the workspace is greenfield (no canonical committed yet).
 */
export async function listCanonicalVersions(): Promise<OntologyVersionsResponse> {
  return request<OntologyVersionsResponse>("/ontology/versions");
}

/**
 * Diff a draft's in-flight ontology against the workspace's
 * canonical head. Greenfield (no canonical yet) renders every
 * entity as an addition.
 */
export async function diffDraftAgainstCanonical(
  draftId: string,
): Promise<OntologyDiffSummary> {
  return request<OntologyDiffSummary>(
    `/ontology-drafts/${encodeURIComponent(draftId)}/diff/canonical`,
  );
}

/**
 * Pin a draft's `parent_version_id` to the workspace canonical
 * head. The MVP rebase moves the parent pointer — conflict
 * detection rides on the existing complete-draft guard.
 */
export async function rebaseDraftAgainstCanonical(
  draftId: string,
): Promise<RebaseDraftResponse> {
  return request<RebaseDraftResponse>(
    `/ontology-drafts/${encodeURIComponent(draftId)}/rebase`,
    { method: "POST" },
  );
}

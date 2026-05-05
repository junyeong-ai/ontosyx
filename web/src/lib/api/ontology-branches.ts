import { request } from "./client";
import type {
  OntologyDiffSummary,
  OntologyVersionsResponse,
  RebaseDraftResponse,
  RebasePreviewResponse,
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
 * Read-only rebase analysis — conflicts, base→head and
 * base→draft diffs, current canonical head id. Drives the
 * preview surface; the FE renders conflicts before the operator
 * commits to the pin.
 */
export async function previewRebaseAgainstCanonical(
  draftId: string,
): Promise<RebasePreviewResponse> {
  return request<RebasePreviewResponse>(
    `/ontology-drafts/${encodeURIComponent(draftId)}/rebase/preview`,
  );
}

/**
 * Pin a draft's `parent_version_id` to the workspace canonical
 * head. When `acknowledge_conflicts` is `false` (default), a
 * non-empty conflict surface returns 409 — the FE routes the
 * operator to the preview, then resubmits with `true` once the
 * operator has reconciled.
 */
export async function rebaseDraftAgainstCanonical(
  draftId: string,
  acknowledgeConflicts = false,
): Promise<RebaseDraftResponse> {
  return request<RebaseDraftResponse>(
    `/ontology-drafts/${encodeURIComponent(draftId)}/rebase`,
    {
      method: "POST",
      body: JSON.stringify({
        acknowledge_conflicts: acknowledgeConflicts,
      }),
    },
  );
}

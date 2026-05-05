import { request } from "./client";
import type { OntologyVersionsResponse } from "@/types/ontology-branches";

/**
 * Workspace canonical version history. Newest first. Empty when
 * the workspace is greenfield (no canonical committed yet).
 */
export async function listCanonicalVersions(): Promise<OntologyVersionsResponse> {
  return request<OntologyVersionsResponse>("/ontology/versions");
}

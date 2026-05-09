// Ambiguity admin API client.

import type { components } from "@/types/api.generated";
import { request } from "./client";

export type AmbiguityContext = components["schemas"]["AmbiguityContext"];
export type AmbiguityMapping = components["schemas"]["AmbiguityMapping"];
export type AmbiguityResolution = components["schemas"]["AmbiguityResolution"];
export type AmbiguitySummary = components["schemas"]["AmbiguitySummary"];
export type AmbiguityListResponse = components["schemas"]["AmbiguityListResponse"];
export type AmbiguityDetailResponse =
  components["schemas"]["AmbiguityDetailResponse"];

export async function listAmbiguities(): Promise<AmbiguityListResponse> {
  return request(`/ambiguities`);
}

export async function getAmbiguity(
  id: string,
): Promise<AmbiguityDetailResponse> {
  return request(`/ambiguities/${encodeURIComponent(id)}`);
}

export async function resolveAmbiguity(
  id: string,
  mapping: AmbiguityMapping,
): Promise<AmbiguityResolution> {
  return request(`/ambiguities/${encodeURIComponent(id)}/resolve`, {
    method: "POST",
    body: JSON.stringify({ mapping }),
  });
}

export async function revokeAmbiguity(id: string): Promise<{ revoked: boolean }> {
  return request(`/ambiguities/${encodeURIComponent(id)}/revoke`, {
    method: "POST",
  });
}

export async function bulkRevokeAmbiguities(
  ids: readonly string[],
): Promise<{ revoked: number }> {
  return request(`/ambiguities/bulk-revoke`, {
    method: "POST",
    body: JSON.stringify({ ids }),
  });
}

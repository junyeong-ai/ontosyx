import { request } from "./client";
import type { components } from "@/types/api.generated";

export type CommunitySummary = components["schemas"]["CommunitySummaryDto"];
export type UpsertCommunitySummaryRequest =
  components["schemas"]["UpsertCommunitySummaryRequest"];
export type CommunitySummaryResponse =
  components["schemas"]["CommunitySummaryResponse"];
export type ListCommunitySummariesResponse =
  components["schemas"]["ListCommunitySummariesResponse"];

export interface SearchCommunitySummariesParams {
  q: string;
  topK?: number;
}

export async function listCommunitySummaries(): Promise<ListCommunitySummariesResponse> {
  return request("/ontology/communities");
}

export async function upsertCommunitySummary(
  body: UpsertCommunitySummaryRequest,
): Promise<CommunitySummaryResponse> {
  return request("/ontology/communities", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function searchCommunitySummaries({
  q,
  topK,
}: SearchCommunitySummariesParams): Promise<ListCommunitySummariesResponse> {
  const params = new URLSearchParams({ q });
  if (topK !== undefined) params.set("top_k", String(topK));
  return request(`/ontology/communities/search?${params.toString()}`);
}

export async function deleteCommunitySummary(id: string): Promise<void> {
  await request(`/ontology/communities/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

import type {
  CursorPage,
  KnowledgeEntry,
  KnowledgeCreateRequest,
  KnowledgeUpdateRequest,
  KnowledgeStats,
} from "@/types/api";
import { request } from "./client";

export async function listKnowledge(params?: {
  ontology_name?: string;
  kind?: string;
  status?: string;
  limit?: number;
  cursor?: string;
}): Promise<CursorPage<KnowledgeEntry>> {
  const qs = new URLSearchParams();
  if (params?.ontology_name) qs.set("ontology_name", params.ontology_name);
  if (params?.kind) qs.set("kind", params.kind);
  if (params?.status) qs.set("status", params.status);
  if (params?.limit) qs.set("limit", String(params.limit));
  if (params?.cursor) qs.set("cursor", params.cursor);
  const query = qs.toString();
  return request<CursorPage<KnowledgeEntry>>(`/knowledge${query ? `?${query}` : ""}`);
}

export async function getKnowledge(id: string): Promise<KnowledgeEntry> {
  return request<KnowledgeEntry>(`/knowledge/${id}`);
}

export async function createKnowledge(
  req: KnowledgeCreateRequest,
): Promise<{ id: string }> {
  return request<{ id: string }>("/knowledge", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateKnowledge(
  id: string,
  req: KnowledgeUpdateRequest,
): Promise<void> {
  await request(`/knowledge/${id}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteKnowledge(id: string): Promise<void> {
  await request(`/knowledge/${id}`, { method: "DELETE" });
}

export async function updateKnowledgeStatus(
  id: string,
  status: string,
  reviewNotes?: string,
): Promise<void> {
  await request(`/knowledge/${id}/status`, {
    method: "PATCH",
    body: JSON.stringify({ status, review_notes: reviewNotes }),
  });
}

export async function listStaleKnowledge(): Promise<CursorPage<KnowledgeEntry>> {
  return request<CursorPage<KnowledgeEntry>>("/knowledge/stale");
}

export async function knowledgeStats(): Promise<KnowledgeStats> {
  return request<KnowledgeStats>("/knowledge/stats");
}

export async function bulkReviewKnowledge(
  ids: string[],
  status: string,
  reviewNotes?: string,
): Promise<{ reviewed: number }> {
  return request<{ reviewed: number }>("/knowledge/bulk-review", {
    method: "POST",
    body: JSON.stringify({ ids, status, review_notes: reviewNotes }),
  });
}

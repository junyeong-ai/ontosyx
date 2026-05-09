// Φ11 — Verified-query bank API client.
//
// Five endpoints under `/verified-queries`:
//
// - POST   /verified-queries                    promote
// - GET    /verified-queries?status=...         list (designer admin surface)
// - GET    /verified-queries/{id}               detail
// - POST   /verified-queries/{id}/transition-status   lifecycle transition
// - DELETE /verified-queries/{id}               hard delete
//
// All admin-gated server-side (`require_designer`); the FE surface
// hides the actions from non-designer roles via `useAuth().isAdmin`.

import type {
  PromoteVerifiedQueryRequest,
  TransitionVerifiedQueryStatusRequest,
  VerifiedQuery,
  VerifiedQueryId,
  VerifiedQueryListResponse,
  VerifiedQueryStatus,
} from "@/types/api";
import { request } from "./client";

export async function listVerifiedQueries(params?: {
  status?: VerifiedQueryStatus;
  limit?: number;
}): Promise<VerifiedQueryListResponse> {
  const qs = new URLSearchParams();
  if (params?.status) qs.set("status", params.status);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request<VerifiedQueryListResponse>(
    `/verified-queries${query ? `?${query}` : ""}`,
  );
}

export async function getVerifiedQuery(id: VerifiedQueryId): Promise<VerifiedQuery> {
  return request<VerifiedQuery>(`/verified-queries/${encodeURIComponent(id)}`);
}

export async function promoteVerifiedQuery(
  body: PromoteVerifiedQueryRequest,
): Promise<VerifiedQuery> {
  return request<VerifiedQuery>("/verified-queries", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function transitionVerifiedQueryStatus(
  id: VerifiedQueryId,
  body: TransitionVerifiedQueryStatusRequest,
): Promise<VerifiedQuery> {
  return request<VerifiedQuery>(
    `/verified-queries/${encodeURIComponent(id)}/transition-status`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

export async function deleteVerifiedQuery(id: VerifiedQueryId): Promise<void> {
  await request(`/verified-queries/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

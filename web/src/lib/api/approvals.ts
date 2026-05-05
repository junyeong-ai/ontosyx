// API client for the approvals admin surface — list, single-id
// review, bulk review, and the per-row comment thread.

import type { components } from "@/types/api.generated";
import { request } from "./client";

export type ApprovalRequest = components["schemas"]["ApprovalRequest"];
export type ApprovalComment = components["schemas"]["ApprovalComment"];

export async function listApprovals(): Promise<ApprovalRequest[]> {
  const data = await request<ApprovalRequest[]>("/approvals");
  return Array.isArray(data) ? data : [];
}

export async function reviewApproval(
  id: string,
  approved: boolean,
  note?: string,
): Promise<{ status: string }> {
  return request<{ status: string }>(
    `/approvals/${encodeURIComponent(id)}/review`,
    {
      method: "POST",
      body: JSON.stringify({ approved, note }),
    },
  );
}

export async function bulkReviewApprovals(
  ids: string[],
  approved: boolean,
  note?: string,
): Promise<{ reviewed: number }> {
  return request<{ reviewed: number }>("/approvals/bulk-review", {
    method: "POST",
    body: JSON.stringify({ ids, approved, note }),
  });
}

export async function listApprovalComments(
  approvalId: string,
): Promise<ApprovalComment[]> {
  const data = await request<ApprovalComment[]>(
    `/approvals/${encodeURIComponent(approvalId)}/comments`,
  );
  return Array.isArray(data) ? data : [];
}

export async function createApprovalComment(
  approvalId: string,
  body: string,
): Promise<ApprovalComment> {
  return request<ApprovalComment>(
    `/approvals/${encodeURIComponent(approvalId)}/comments`,
    {
      method: "POST",
      body: JSON.stringify({ body }),
    },
  );
}

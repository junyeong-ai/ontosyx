// API client for the approval comment thread.

import type { components } from "@/types/api.generated";
import { request } from "./client";

export type ApprovalComment = components["schemas"]["ApprovalComment"];

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

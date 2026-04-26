// API client for the approval comment thread.

import { request } from "./client";

export interface ApprovalComment {
  id: string;
  workspace_id: string;
  approval_id: string;
  author_id: string;
  /** Display name resolved server-side from `users.name`. NULL for
   *  records whose author has been deleted from the workspace. */
  author_name: string | null;
  body: string;
  created_at: string;
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

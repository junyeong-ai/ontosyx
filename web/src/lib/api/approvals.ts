// API client for approval comment threads — Φ6 #2 proper.
//
// The single-textarea reviewer-note (the interim Φ6 #2 in 71bcff9)
// stayed wired but is now mirrored into this thread on /review so
// pre-/post-decision discussion lives alongside the rationale.

import { request } from "./client";

export interface ApprovalComment {
  id: string;
  workspace_id: string;
  approval_id: string;
  author_id: string;
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

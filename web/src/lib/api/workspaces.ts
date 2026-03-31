import { request } from "./client";
import type {
  WorkspaceSummary,
  Workspace,
  WorkspaceMember,
  CreateWorkspaceRequest,
  UpdateWorkspaceRequest,
  AddMemberRequest,
} from "@/types/workspace";

// ---------------------------------------------------------------------------
// Workspace CRUD
// ---------------------------------------------------------------------------

export async function listWorkspaces(): Promise<WorkspaceSummary[]> {
  return request<WorkspaceSummary[]>("/workspaces");
}

export async function getWorkspace(id: string): Promise<Workspace> {
  return request<Workspace>(`/workspaces/${encodeURIComponent(id)}`);
}

export async function createWorkspace(
  req: CreateWorkspaceRequest,
): Promise<Workspace> {
  return request<Workspace>("/workspaces", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateWorkspace(
  id: string,
  req: UpdateWorkspaceRequest,
): Promise<Workspace> {
  return request<Workspace>(`/workspaces/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteWorkspace(id: string): Promise<void> {
  await request<void>(`/workspaces/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Member management
// ---------------------------------------------------------------------------

export async function listMembers(wsId: string): Promise<WorkspaceMember[]> {
  return request<WorkspaceMember[]>(
    `/workspaces/${encodeURIComponent(wsId)}/members`,
  );
}

export async function addMember(
  wsId: string,
  req: AddMemberRequest,
): Promise<void> {
  await request<void>(
    `/workspaces/${encodeURIComponent(wsId)}/members`,
    { method: "POST", body: JSON.stringify(req) },
  );
}

export async function updateMemberRole(
  wsId: string,
  userId: string,
  role: string,
): Promise<void> {
  await request<void>(
    `/workspaces/${encodeURIComponent(wsId)}/members/${encodeURIComponent(userId)}`,
    { method: "PATCH", body: JSON.stringify({ role }) },
  );
}

export async function removeMember(
  wsId: string,
  userId: string,
): Promise<void> {
  await request<void>(
    `/workspaces/${encodeURIComponent(wsId)}/members/${encodeURIComponent(userId)}`,
    { method: "DELETE" },
  );
}

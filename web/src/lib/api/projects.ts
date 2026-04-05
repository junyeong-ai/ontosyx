import type {
  ProjectReconcileRequest,
  CompleteProjectRequest,
  CreateProjectRequest,
  CursorPage,
  ProjectDeployRequest,
  ProjectDeployResponse,
  DesignProject,
  DesignProjectRequest,
  DesignProjectSummary,
  ProjectDesignResponse,
  ProjectEditRequest,
  ProjectEditResponse,
  ProjectLoadCompileRequest,
  ProjectLoadCompileResponse,
  ProjectExtendRequest,
  ProjectExtendResponse,
  ProjectLoadPlanResponse,
  ProjectMigrateRequest,
  ProjectMigrateResponse,
  OntologyCommand,
  PendingReconcile,
  ProjectReanalyzeRequest,
  RefineProjectRequest,
  ProjectRefineResponse,
  UpdateDecisionsRequest,
} from "@/types/api";
import { getPrincipalId } from "@/lib/principal";
import { getWorkspaceId } from "@/lib/workspace";
import { fetchWithTimeout, PROXY_BASE, DESIGN_TIMEOUT, request } from "./client";
import { consumeSSEStream } from "./sse";
import {
  CursorPageSchema,
  DesignProjectSchema,
  DesignProjectSummarySchema,
} from "@/lib/validation";

// ---------------------------------------------------------------------------
// Project CRUD
// ---------------------------------------------------------------------------

export async function createProject(
  req: CreateProjectRequest,
): Promise<DesignProject> {
  return request("/projects", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listProjects(params?: {
  cursor?: string;
  limit?: number;
}): Promise<CursorPage<DesignProjectSummary>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  const data = await request(`/projects${query ? `?${query}` : ""}`);
  const result = CursorPageSchema(DesignProjectSummarySchema).safeParse(data);
  if (!result.success) {
    console.warn("Project list validation failed:", result.error.issues);
    return data as ReturnType<typeof CursorPageSchema<typeof DesignProjectSummarySchema>>["_output"];
  }
  return result.data;
}

export async function getProject(id: string): Promise<DesignProject> {
  const data = await request(`/projects/${encodeURIComponent(id)}`);
  const result = DesignProjectSchema.safeParse(data);
  if (!result.success) {
    console.warn("Project validation failed:", result.error.issues);
    return data as DesignProject;
  }
  return result.data;
}

export async function deleteProject(id: string): Promise<void> {
  return request(`/projects/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Project mutations
// ---------------------------------------------------------------------------

export async function updateDecisions(
  id: string,
  req: UpdateDecisionsRequest,
): Promise<DesignProject> {
  return request(`/projects/${encodeURIComponent(id)}/decisions`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function reanalyzeProject(
  id: string,
  req: ProjectReanalyzeRequest,
): Promise<{ project: DesignProject; invalidated_decisions?: string[] }> {
  return request(`/projects/${encodeURIComponent(id)}/reanalyze`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function refineProject(
  id: string,
  req: RefineProjectRequest,
): Promise<ProjectRefineResponse> {
  return request(`/projects/${encodeURIComponent(id)}/refine`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function editProject(
  projectId: string,
  req: ProjectEditRequest,
): Promise<ProjectEditResponse> {
  return request(`/projects/${encodeURIComponent(projectId)}/edit`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function applyReconcile(
  projectId: string,
  req: ProjectReconcileRequest,
): Promise<ProjectRefineResponse> {
  return request(`/projects/${encodeURIComponent(projectId)}/apply-reconcile`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function extendProject(
  id: string,
  req: ProjectExtendRequest,
): Promise<ProjectExtendResponse> {
  return request(`/projects/${encodeURIComponent(id)}/extend`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function completeProject(
  id: string,
  req: CompleteProjectRequest,
): Promise<DesignProject> {
  return request(`/projects/${encodeURIComponent(id)}/complete`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Ontology Commands (save boundary)
// ---------------------------------------------------------------------------

export async function applyOntologyCommands(
  projectId: string,
  req: { revision: number; commands: OntologyCommand[] },
): Promise<{ project: DesignProject }> {
  return request(`/projects/${encodeURIComponent(projectId)}/ontology`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Schema Deploy
// ---------------------------------------------------------------------------

export async function deploySchema(
  id: string,
  req: ProjectDeployRequest,
): Promise<ProjectDeployResponse> {
  return request(`/projects/${encodeURIComponent(id)}/deploy-schema`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Schema Migration
// ---------------------------------------------------------------------------

export async function migrateSchema(
  projectId: string,
  revision: number,
  req: ProjectMigrateRequest,
): Promise<ProjectMigrateResponse> {
  return request(
    `/projects/${encodeURIComponent(projectId)}/revisions/${revision}/migrate`,
    {
      method: "POST",
      body: JSON.stringify(req),
    },
  );
}

// ---------------------------------------------------------------------------
// Load Plan
// ---------------------------------------------------------------------------

export async function generateLoadPlan(
  id: string,
): Promise<ProjectLoadPlanResponse> {
  return request(`/projects/${encodeURIComponent(id)}/load-plan`, {
    method: "POST",
  });
}

export async function compileLoad(
  id: string,
  req: ProjectLoadCompileRequest,
): Promise<ProjectLoadCompileResponse> {
  return request(`/projects/${encodeURIComponent(id)}/load/compile`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Design/Refine SSE Streaming
// ---------------------------------------------------------------------------

export interface DesignStreamCallbacks {
  onPhase?: (phase: string, detail?: string) => void;
  onResult?: (result: ProjectDesignResponse) => void;
  onError?: (errorType: string, message: string) => void;
}

export interface RefineStreamCallbacks {
  onPhase?: (phase: string, detail?: string) => void;
  onResult?: (result: ProjectRefineResponse) => void;
  onUncertainReconcile?: (data: PendingReconcile) => void;
  onError?: (errorType: string, message: string) => void;
}

async function consumeProjectStream(
  url: string,
  body: string,
  callbacks: {
    onPhase?: (phase: string, detail?: string) => void;
    onResult?: (data: unknown) => void;
    onUncertainReconcile?: (data: unknown) => void;
    onError?: (errorType: string, message: string) => void;
  },
): Promise<void> {
  const headers = new Headers({ "Content-Type": "application/json" });
  const principalId = getPrincipalId();
  if (principalId) {
    headers.set("x-principal-id", principalId);
  }
  const workspaceId = getWorkspaceId();
  if (workspaceId) {
    headers.set("x-workspace-id", workspaceId);
  }

  const res = await fetchWithTimeout(`${PROXY_BASE}${url}`, {
    method: "POST",
    headers,
    body,
    timeout: DESIGN_TIMEOUT,
  });

  if (!res.ok || !res.body) {
    const respBody = await res.json().catch(() => ({}));
    const msg = respBody.error?.message ?? respBody.error ?? `Stream error ${res.status}`;
    callbacks.onError?.("http_error", msg);
    return;
  }

  await consumeSSEStream(res, {
    phase: (data) => {
      const d = data as { phase: string; detail?: string };
      callbacks.onPhase?.(d.phase, d.detail);
    },
    result: (data) => {
      callbacks.onResult?.(data);
    },
    uncertain_reconcile: (data) => {
      callbacks.onUncertainReconcile?.(data);
    },
    error: (data) => {
      const d = data as { error?: { type?: string; message?: string } };
      callbacks.onError?.(
        d.error?.type ?? "unknown",
        d.error?.message ?? "Unknown error",
      );
    },
  });
}

export async function designProjectStream(
  id: string,
  req: DesignProjectRequest,
  callbacks: DesignStreamCallbacks,
): Promise<void> {
  return consumeProjectStream(
    `/projects/${encodeURIComponent(id)}/design/stream`,
    JSON.stringify(req),
    {
      onPhase: callbacks.onPhase,
      onResult: (data) =>
        callbacks.onResult?.(data as ProjectDesignResponse),
      onError: callbacks.onError,
    },
  );
}

export async function refineProjectStream(
  id: string,
  req: RefineProjectRequest,
  callbacks: RefineStreamCallbacks,
): Promise<void> {
  return consumeProjectStream(
    `/projects/${encodeURIComponent(id)}/refine/stream`,
    JSON.stringify(req),
    {
      onPhase: callbacks.onPhase,
      onResult: (data) =>
        callbacks.onResult?.(data as ProjectRefineResponse),
      onUncertainReconcile: (data) =>
        callbacks.onUncertainReconcile?.(
          data as PendingReconcile,
        ),
      onError: callbacks.onError,
    },
  );
}

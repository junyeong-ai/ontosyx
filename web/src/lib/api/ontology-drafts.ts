import type {
  ReconcileProjectRequest,
  CompleteProjectRequest,
  CreateProjectRequest,
  CursorPage,
  ProjectDeployRequest,
  ProjectDeployResponse,
  OntologyDraft,
  DesignOntologyDraftRequest,
  OntologyDraftSummary,
  DesignOntologyDraftResponse,
  EditProjectRequest,
  EditProjectResponse,
  ProjectLoadCompileRequest,
  ProjectLoadCompileResponse,
  ExtendProjectRequest,
  ExtendProjectResponse,
  ProjectLoadPlanResponse,
  ProjectMigrateRequest,
  ProjectMigrateResponse,
  OntologyCommand,
  PendingReconcile,
  ReanalyzeProjectRequest,
  RefineProjectRequest,
  RefineProjectResponse,
  UpdateProjectDecisionsRequest,
} from "@/types/api";
import type { ProjectSource } from "@/types/ontology-drafts";
import { getPrincipalId } from "@/lib/principal";
import { getWorkspaceId } from "@/lib/workspace";
import { fetchWithTimeout, PROXY_BASE, DESIGN_TIMEOUT, request } from "./client";
import { consumeSSEStream } from "./sse";
import {
  CursorPageSchema,
  OntologyDraftSchema,
  OntologyDraftSummarySchema,
} from "@/lib/validation";

// ---------------------------------------------------------------------------
// Project CRUD
// ---------------------------------------------------------------------------

export async function createOntologyDraft(
  req: CreateProjectRequest,
): Promise<OntologyDraft> {
  return request("/projects", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listOntologyDrafts(params?: {
  cursor?: string;
  limit?: number;
}): Promise<CursorPage<OntologyDraftSummary>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  const data = await request(`/projects${query ? `?${query}` : ""}`);
  const result = CursorPageSchema(OntologyDraftSummarySchema).safeParse(data);
  if (!result.success) {
    console.warn("Project list validation failed:", result.error.issues);
    return data as ReturnType<typeof CursorPageSchema<typeof OntologyDraftSummarySchema>>["_output"];
  }
  return result.data;
}

export async function getOntologyDraft(id: string): Promise<OntologyDraft> {
  const data = await request(`/ontology-drafts/${encodeURIComponent(id)}`);
  const result = OntologyDraftSchema.safeParse(data);
  if (!result.success) {
    console.warn("Project validation failed:", result.error.issues);
    return data as OntologyDraft;
  }
  return result.data;
}

export async function deleteOntologyDraft(id: string): Promise<void> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Project mutations
// ---------------------------------------------------------------------------

export async function updateDecisions(
  id: string,
  req: UpdateProjectDecisionsRequest,
): Promise<OntologyDraft> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/decisions`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function reanalyzeOntologyDraft(
  id: string,
  req: ReanalyzeProjectRequest,
): Promise<{ project: OntologyDraft; invalidated_decisions?: string[] }> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/reanalyze`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

interface ReanalyzeModeledProjectRequest {
  source: ProjectSource;
  revision: number;
  repo_source?: ReanalyzeProjectRequest["repo_source"];
}

export async function reanalyzeModeledProject(
  id: string,
  req: ReanalyzeModeledProjectRequest,
): Promise<{ project: OntologyDraft; invalidated_decisions?: string[] }> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/reanalyze-modeled`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function refineOntologyDraft(
  id: string,
  req: RefineProjectRequest,
): Promise<RefineProjectResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/refine`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function editOntologyDraft(
  ontologyDraftId: string,
  req: EditProjectRequest,
): Promise<EditProjectResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/edit`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function applyReconcile(
  ontologyDraftId: string,
  req: ReconcileProjectRequest,
): Promise<RefineProjectResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/apply-reconcile`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function extendOntologyDraft(
  id: string,
  req: ExtendProjectRequest,
): Promise<ExtendProjectResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/extend`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function completeOntologyDraft(
  id: string,
  req: CompleteProjectRequest,
): Promise<OntologyDraft> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/complete`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export interface IncludeScopeTablesRequest {
  tables: string[];
  expected_revision: number;
}

export interface DeferScopeTablesRequest {
  tables: string[];
  reason: string;
  expected_revision: number;
}

export interface ScopeUpdateResponse {
  project: OntologyDraft;
}

export async function includeScopeTables(
  id: string,
  req: IncludeScopeTablesRequest,
): Promise<ScopeUpdateResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/scope/include`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function deferScopeTables(
  id: string,
  req: DeferScopeTablesRequest,
): Promise<ScopeUpdateResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/scope/defer`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Ontology Commands (save boundary)
// ---------------------------------------------------------------------------

export async function applyOntologyCommands(
  ontologyDraftId: string,
  req: { revision: number; commands: OntologyCommand[] },
): Promise<{ project: OntologyDraft }> {
  return request(`/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/ontology`, {
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
  return request(`/ontology-drafts/${encodeURIComponent(id)}/deploy-schema`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Schema Migration
// ---------------------------------------------------------------------------

export async function migrateSchema(
  ontologyDraftId: string,
  revision: number,
  req: ProjectMigrateRequest,
): Promise<ProjectMigrateResponse> {
  return request(
    `/ontology-drafts/${encodeURIComponent(ontologyDraftId)}/revisions/${revision}/migrate`,
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
  return request(`/ontology-drafts/${encodeURIComponent(id)}/load-plan`, {
    method: "POST",
  });
}

export async function compileLoad(
  id: string,
  req: ProjectLoadCompileRequest,
): Promise<ProjectLoadCompileResponse> {
  return request(`/ontology-drafts/${encodeURIComponent(id)}/load/compile`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// Design/Refine SSE Streaming
// ---------------------------------------------------------------------------

export interface DesignStreamCallbacks {
  onPhase?: (phase: string, detail?: string) => void;
  onResult?: (result: DesignOntologyDraftResponse) => void;
  onError?: (errorType: string, message: string) => void;
}

export interface RefineStreamCallbacks {
  onPhase?: (phase: string, detail?: string) => void;
  onResult?: (result: RefineProjectResponse) => void;
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

export async function designOntologyDraftStream(
  id: string,
  req: DesignOntologyDraftRequest,
  callbacks: DesignStreamCallbacks,
): Promise<void> {
  return consumeProjectStream(
    `/ontology-drafts/${encodeURIComponent(id)}/design/stream`,
    JSON.stringify(req),
    {
      onPhase: callbacks.onPhase,
      onResult: (data) =>
        callbacks.onResult?.(data as DesignOntologyDraftResponse),
      onError: callbacks.onError,
    },
  );
}

export async function refineOntologyDraftStream(
  id: string,
  req: RefineProjectRequest,
  callbacks: RefineStreamCallbacks,
): Promise<void> {
  return consumeProjectStream(
    `/ontology-drafts/${encodeURIComponent(id)}/refine/stream`,
    JSON.stringify(req),
    {
      onPhase: callbacks.onPhase,
      onResult: (data) =>
        callbacks.onResult?.(data as RefineProjectResponse),
      onUncertainReconcile: (data) =>
        callbacks.onUncertainReconcile?.(
          data as PendingReconcile,
        ),
      onError: callbacks.onError,
    },
  );
}

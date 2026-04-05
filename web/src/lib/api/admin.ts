import type {
  AgentEvent,
  AgentSession,
  AnalysisRecipe,
  ConfigResponse,
  ConfigUpdateRequest,
  CursorPage,
  HealthResponse,
  PromptTemplate,
  QueryResult,
  ScheduledTask,
  SessionMessage,
  UiConfig,
  UserInfo,
  ReportCreateRequest,
  SavedReport,
  ReportUpdateRequest,
} from "@/types/api";
import { request } from "./client";
import { normalizeQueryResult } from "./normalization";

// ---------------------------------------------------------------------------
// Health & Config
// ---------------------------------------------------------------------------

export async function getHealth(): Promise<HealthResponse> {
  return request("/health", { maxRetries: 0 });
}

export async function getUiConfig(): Promise<UiConfig> {
  return request("/config/ui");
}

export async function getConfig(): Promise<ConfigResponse> {
  return request("/config");
}

export async function updateConfig(
  req: ConfigUpdateRequest,
): Promise<{ updated: number }> {
  return request("/config", {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

// ---------------------------------------------------------------------------
// User Management
// ---------------------------------------------------------------------------

export async function listUsers(params?: {
  cursor?: string;
  limit?: number;
}): Promise<CursorPage<UserInfo>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request(`/users${query ? `?${query}` : ""}`);
}

export async function updateUserRole(
  id: string,
  role: string,
): Promise<{ user: UserInfo }> {
  return request(`/users/${encodeURIComponent(id)}/role`, {
    method: "PATCH",
    body: JSON.stringify({ role }),
  });
}

// ---------------------------------------------------------------------------
// Prompt Templates (Admin)
// ---------------------------------------------------------------------------

export async function listPromptTemplates(): Promise<PromptTemplate[]> {
  return request("/admin/prompts");
}

export async function getPromptTemplate(id: string): Promise<PromptTemplate> {
  return request(`/admin/prompts/${encodeURIComponent(id)}`);
}

export async function createPromptTemplate(req: {
  name: string;
  version: string;
  content: string;
  variables?: unknown[];
  metadata?: Record<string, unknown>;
}): Promise<PromptTemplate> {
  return request("/admin/prompts", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updatePromptTemplate(
  id: string,
  req: { content?: string; variables?: unknown[]; is_active?: boolean },
): Promise<void> {
  await request(`/admin/prompts/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deletePromptTemplate(id: string): Promise<void> {
  await request(`/admin/prompts/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Agent Sessions (Audit)
// ---------------------------------------------------------------------------

export async function listAgentSessions(params?: {
  limit?: number;
  cursor?: string;
}): Promise<CursorPage<AgentSession>> {
  const search = new URLSearchParams();
  if (params?.limit) search.set("limit", String(params.limit));
  if (params?.cursor) search.set("cursor", params.cursor);
  const qs = search.toString();
  return request(`/sessions${qs ? `?${qs}` : ""}`);
}

export async function getAgentSession(id: string): Promise<AgentSession> {
  return request(`/sessions/${encodeURIComponent(id)}`);
}

export async function listAgentEvents(sessionId: string): Promise<AgentEvent[]> {
  return request(`/sessions/${encodeURIComponent(sessionId)}/events`);
}

export async function fetchSessionMessages(sessionId: string): Promise<{ messages: SessionMessage[] }> {
  return request(`/sessions/${encodeURIComponent(sessionId)}/messages`);
}

export async function deleteSession(sessionId: string): Promise<void> {
  await request(`/sessions/${encodeURIComponent(sessionId)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// HITL Tool Review
// ---------------------------------------------------------------------------

export async function respondToolReview(
  sessionId: string,
  toolCallId: string,
  approved: boolean,
): Promise<void> {
  await request(`/sessions/${encodeURIComponent(sessionId)}/tools/${encodeURIComponent(toolCallId)}/respond`, {
    method: "POST",
    body: JSON.stringify({ approved }),
  });
}

// ---------------------------------------------------------------------------
// Recipes
// ---------------------------------------------------------------------------

export async function listRecipes(params?: {
  limit?: number;
  cursor?: string;
}): Promise<CursorPage<AnalysisRecipe>> {
  const search = new URLSearchParams();
  if (params?.limit) search.set("limit", String(params.limit));
  if (params?.cursor) search.set("cursor", params.cursor);
  const qs = search.toString();
  return request(`/recipes${qs ? `?${qs}` : ""}`);
}

export async function createRecipe(
  req: Omit<AnalysisRecipe, "id" | "created_by" | "created_at">,
): Promise<AnalysisRecipe> {
  return request("/recipes", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

export async function deleteRecipe(id: string): Promise<void> {
  await request(`/recipes/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export async function listRecipeVersions(
  recipeId: string,
): Promise<AnalysisRecipe[]> {
  return request(`/recipes/${encodeURIComponent(recipeId)}/versions`);
}

export async function createRecipeVersion(
  recipeId: string,
  req: Omit<AnalysisRecipe, "id" | "created_by" | "created_at" | "version" | "status" | "parent_id">,
): Promise<AnalysisRecipe> {
  return request(`/recipes/${encodeURIComponent(recipeId)}/versions`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateRecipeStatus(
  recipeId: string,
  status: "draft" | "approved" | "deprecated",
): Promise<void> {
  await request(`/recipes/${encodeURIComponent(recipeId)}/status`, {
    method: "PATCH",
    body: JSON.stringify({ status }),
  });
}

// ---------------------------------------------------------------------------
// Saved Reports
// ---------------------------------------------------------------------------

export async function createReport(
  req: ReportCreateRequest,
): Promise<SavedReport> {
  return request("/reports", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listReports(params: {
  ontology_id: string;
  limit?: number;
  cursor?: string;
}): Promise<CursorPage<SavedReport>> {
  const qs = new URLSearchParams();
  qs.set("ontology_id", params.ontology_id);
  if (params.limit) qs.set("limit", String(params.limit));
  if (params.cursor) qs.set("cursor", params.cursor);
  return request(`/reports?${qs.toString()}`);
}

export async function getReport(
  id: string,
): Promise<SavedReport> {
  return request(`/reports/${encodeURIComponent(id)}`);
}

export async function updateReport(
  id: string,
  req: ReportUpdateRequest,
): Promise<SavedReport> {
  return request(`/reports/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteReport(id: string): Promise<void> {
  await request(`/reports/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function executeReport(
  id: string,
  params: Record<string, unknown>,
): Promise<QueryResult> {
  const raw = await request<Record<string, unknown>>(
    `/reports/${encodeURIComponent(id)}/execute`,
    {
      method: "POST",
      body: JSON.stringify(params),
    },
  );
  return normalizeQueryResult(raw) ?? { columns: [], rows: [] };
}

// ---------------------------------------------------------------------------
// Scheduled Tasks
// ---------------------------------------------------------------------------

export async function listScheduledTasks(params?: {
  recipe_id?: string;
}): Promise<ScheduledTask[]> {
  const qs = new URLSearchParams();
  if (params?.recipe_id) qs.set("recipe_id", params.recipe_id);
  const query = qs.toString();
  return request(`/scheduled-tasks${query ? `?${query}` : ""}`);
}

export async function getScheduledTask(id: string): Promise<ScheduledTask> {
  return request(`/scheduled-tasks/${encodeURIComponent(id)}`);
}

export async function updateScheduledTask(
  id: string,
  req: { enabled?: boolean; cron_expression?: string; description?: string },
): Promise<void> {
  await request(`/scheduled-tasks/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteScheduledTask(id: string): Promise<void> {
  await request(`/scheduled-tasks/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

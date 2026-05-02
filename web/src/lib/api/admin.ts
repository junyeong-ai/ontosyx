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
import type { components } from "@/types/api.generated";
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

// `version`, `status`, and `parent_id` live on the response
// (`AnalysisRecipe`) but the create handler doesn't accept them — the
// generated wire type already excludes them, so it's the honest source.
export type CreateRecipeRequest =
  components["schemas"]["CreateRecipeRequest"];

export async function createRecipe(
  req: CreateRecipeRequest,
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
  ontology_lineage_id: string;
  limit?: number;
  cursor?: string;
}): Promise<CursorPage<SavedReport>> {
  const qs = new URLSearchParams();
  qs.set("ontology_lineage_id", params.ontology_lineage_id);
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

// ---------------------------------------------------------------------------
// Federation adapters (VOL)
// ---------------------------------------------------------------------------

export type FederationAdapterSummary = {
  source_id: string;
  source_type: string;
};

// Discriminated on `kind`. `inline` carries the raw value; `secret_ref`
// carries an opaque reference string (`env:VAR_NAME` today, with
// `vault:` / `aws-sm:` coming later). The server rejects any shape
// that does not match one of these variants at deserialization time,
// so the frontend never has to enforce the "exactly one" invariant.
export type Credential =
  | { kind: "inline"; value: string }
  | { kind: "secret_ref"; value: string };

// GET response omits inline values by design — an inline credential
// surfaces only as `{kind: "inline"}` with no `value`, so curious
// clients cannot read back a raw secret.
export type CredentialSource =
  | { kind: "inline" }
  | { kind: "secret_ref"; value: string };

// GET response shape. Mirrors RegisterFederationAdapterRequest
// exactly, except `credential` is the redacted CredentialSource
// (inline values never echoed back). The outer `kind` tag and
// the variant-specific fields (schema_name where present) are
// flat at the object's top level via serde(flatten) on the server
// side, so the wire form round-trips with the register request.
export type FederationAdapterDetail = { source_id: string } & (
  | { kind: "csv"; credential: CredentialSource }
  | { kind: "json"; credential: CredentialSource }
  | { kind: "postgres"; credential: CredentialSource; schema_name?: string }
  | { kind: "mysql"; credential: CredentialSource; schema_name: string }
  | { kind: "bigquery"; credential: CredentialSource }
);

export type FederationHealthResponse = {
  workspace_id: string;
  resolver_hydrated: boolean;
  resolver_count: number;
  store_count: number;
  in_sync: boolean;
  orphans_in_resolver: string[];
  missing_from_resolver: string[];
};

export type RegisterFederationAdapterRequest = { source_id: string } & (
  | { kind: "csv"; credential: Credential }
  | { kind: "json"; credential: Credential }
  | { kind: "postgres"; credential: Credential; schema_name?: string }
  | { kind: "mysql"; credential: Credential; schema_name: string }
  | { kind: "bigquery"; credential: Credential }
);

export type RegisterFederationAdapterResponse = {
  replaced: boolean;
  adapter: FederationAdapterSummary;
};

export async function listFederationAdapters(): Promise<FederationAdapterSummary[]> {
  return request("/admin/federation/adapters");
}

export async function getFederationAdapter(
  sourceId: string,
): Promise<FederationAdapterDetail> {
  return request(`/admin/federation/adapters/${encodeURIComponent(sourceId)}`);
}

export async function registerFederationAdapter(
  req: RegisterFederationAdapterRequest,
): Promise<RegisterFederationAdapterResponse> {
  return request("/admin/federation/adapters", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function deleteFederationAdapter(sourceId: string): Promise<void> {
  await request(`/admin/federation/adapters/${encodeURIComponent(sourceId)}`, {
    method: "DELETE",
  });
}

export async function refreshFederationAdapters(): Promise<{
  refreshed: boolean;
  count: number;
}> {
  return request("/admin/federation/adapters/refresh", {
    method: "POST",
  });
}

export async function getFederationHealth(): Promise<FederationHealthResponse> {
  return request("/admin/federation/health");
}

// ---------------------------------------------------------------------------
// Preview adapter — dry-run schema introspection before registering. Uses
// the same discriminated-union body as Register minus the `source_id`
// (the server doesn't persist anything, just builds a transient adapter
// and calls `list_tables` + `describe_table`).
// ---------------------------------------------------------------------------

export type PreviewFederationAdapterRequest =
  | { kind: "csv"; credential: Credential }
  | { kind: "json"; credential: Credential }
  | { kind: "postgres"; credential: Credential; schema_name?: string }
  | { kind: "mysql"; credential: Credential; schema_name: string }
  | { kind: "bigquery"; credential: Credential };

export type PreviewFederationColumn = {
  name: string;
  data_type: string;
  nullable: boolean;
};

export type PreviewFederationTable = {
  name: string;
  columns: PreviewFederationColumn[];
};

export type PreviewFederationAdapterResponse = {
  source_type: string;
  tables: PreviewFederationTable[];
};

export async function previewFederationAdapter(
  req: PreviewFederationAdapterRequest,
): Promise<PreviewFederationAdapterResponse> {
  return request("/admin/federation/adapters/preview", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

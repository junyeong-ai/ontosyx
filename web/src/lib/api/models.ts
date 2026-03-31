import { request } from "./client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ModelConfig {
  id: string;
  workspace_id: string | null;
  name: string;
  provider: string;
  model_id: string;
  max_tokens: number;
  temperature: number | null;
  timeout_secs: number;
  cost_per_1m_input: number | null;
  cost_per_1m_output: number | null;
  daily_budget_usd: number | null;
  priority: number;
  enabled: boolean;
  api_key_env: string | null;
  region: string | null;
  base_url: string | null;
  created_at: string;
  updated_at: string;
}

export interface ModelRoutingRule {
  id: string;
  workspace_id: string | null;
  operation: string;
  model_config_id: string;
  priority: number;
  enabled: boolean;
  created_at: string;
}

export interface ModelTestResult {
  success: boolean;
  latency_ms: number;
  model_id: string;
  error: string | null;
}

// ---------------------------------------------------------------------------
// Model Configs
// ---------------------------------------------------------------------------

export async function listModelConfigs(): Promise<ModelConfig[]> {
  return request<ModelConfig[]>("/models/configs");
}

export async function createModelConfig(
  req: Omit<ModelConfig, "id" | "workspace_id" | "created_at" | "updated_at">,
): Promise<ModelConfig> {
  return request<ModelConfig>("/models/configs", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateModelConfig(
  id: string,
  req: Partial<Omit<ModelConfig, "id" | "workspace_id" | "created_at" | "updated_at">>,
): Promise<ModelConfig> {
  return request<ModelConfig>(`/models/configs/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteModelConfig(id: string): Promise<void> {
  await request<void>(`/models/configs/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Routing Rules
// ---------------------------------------------------------------------------

export async function listRoutingRules(): Promise<ModelRoutingRule[]> {
  return request<ModelRoutingRule[]>("/models/routing-rules");
}

export async function createRoutingRule(
  req: Omit<ModelRoutingRule, "id" | "workspace_id" | "created_at">,
): Promise<ModelRoutingRule> {
  return request<ModelRoutingRule>("/models/routing-rules", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateRoutingRule(
  id: string,
  req: Partial<Omit<ModelRoutingRule, "id" | "workspace_id" | "created_at">>,
): Promise<ModelRoutingRule> {
  return request<ModelRoutingRule>(`/models/routing-rules/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteRoutingRule(id: string): Promise<void> {
  await request<void>(`/models/routing-rules/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

export async function testModelConfig(id: string): Promise<ModelTestResult> {
  return request<ModelTestResult>("/models/test", {
    method: "POST",
    body: JSON.stringify({ model_config_id: id }),
  });
}

import type { components } from "@/types/api.generated";
import { request } from "./client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ModelConfig = components["schemas"]["ModelConfig"];
export type NewModelConfig = components["schemas"]["NewModelConfig"];
export type ModelConfigUpdate = components["schemas"]["ModelConfigUpdate"];
export type ModelRoutingRule = components["schemas"]["ModelRoutingRule"];
export type NewRoutingRule = components["schemas"]["NewRoutingRule"];
export type RoutingRuleUpdate = components["schemas"]["RoutingRuleUpdate"];
export type TestModelRequest = components["schemas"]["TestModelRequest"];
export type TestModelResponse = components["schemas"]["TestModelResponse"];
export type ModelOperation = components["schemas"]["ModelOperation"];

// ---------------------------------------------------------------------------
// Operation Registry
// ---------------------------------------------------------------------------

export async function listModelOperations(): Promise<ModelOperation[]> {
  return request<ModelOperation[]>("/models/operations");
}

// ---------------------------------------------------------------------------
// Model Configs
// ---------------------------------------------------------------------------

export async function listModelConfigs(): Promise<ModelConfig[]> {
  return request<ModelConfig[]>("/models/configs");
}

export async function createModelConfig(
  req: NewModelConfig,
): Promise<ModelConfig> {
  return request<ModelConfig>("/models/configs", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateModelConfig(
  id: string,
  req: ModelConfigUpdate,
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
  req: NewRoutingRule,
): Promise<ModelRoutingRule> {
  return request<ModelRoutingRule>("/models/routing-rules", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateRoutingRule(
  id: string,
  req: RoutingRuleUpdate,
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

export async function testModelConfig(req: TestModelRequest): Promise<TestModelResponse> {
  return request<TestModelResponse>("/models/test", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

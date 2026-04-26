// API client for `/api/admin/governance/routing` — workspace
// override CRUD. The runtime resolution path runs on every
// OntologyEditOp; this client surfaces the override matrix for the
// admin UI to render + edit.

import type { components } from "@/types/api.generated";
import { request } from "./client";

export type RiskLevel = components["schemas"]["RiskLevel"];
export type ApprovalRouting = components["schemas"]["ApprovalRouting"];
export type ChangeRoutingRule = components["schemas"]["ChangeRoutingRuleResponse"];
export type UpsertRoutingRuleRequest = components["schemas"]["UpsertRoutingRuleRequest"];

export async function listRoutingRules(): Promise<ChangeRoutingRule[]> {
  return request<ChangeRoutingRule[]>("/admin/governance/routing");
}

export async function upsertRoutingRule(
  changeType: string,
  body: UpsertRoutingRuleRequest,
): Promise<ChangeRoutingRule> {
  return request<ChangeRoutingRule>(
    `/admin/governance/routing/${encodeURIComponent(changeType)}`,
    { method: "PUT", body: JSON.stringify(body) },
  );
}

export async function deleteRoutingRule(changeType: string): Promise<void> {
  await request(`/admin/governance/routing/${encodeURIComponent(changeType)}`, {
    method: "DELETE",
  });
}

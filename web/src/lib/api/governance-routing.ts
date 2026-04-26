// API client for `/api/admin/governance/routing` — Φ6 #1.
//
// The routing matrix runs at `state.store.resolve_change_routing`
// every time an OntologyEditOp lands. This client surfaces the
// per-workspace overrides for the admin UI to render + edit. The
// underlying `ApprovalRouting` shape is a pass-through (its enum
// shape is open enough that re-declaring it on the FE would
// duplicate the canonical Rust definition without adding type
// safety).

import { request } from "./client";

/** Risk badge — UI-only metadata. Does not influence routing
 *  itself; that's `routing`'s job. */
export type RiskLevel = "low" | "medium" | "high";

/** Wire-format mirror of `ox_ontology::change_routing::ApprovalRouting`.
 *  Pass-through `Record<string, unknown>` for variants that carry
 *  nested predicates / role lists; the FE renders the variant tag
 *  + a JSON dump of the body until a richer editor lands. */
export type ApprovalRouting =
  | { kind: "auto_approve" }
  | { kind: "auto_approve_with_notification"; notify_roles: string[] }
  | {
      kind: "approval_required_unless";
      skip_predicates: Array<Record<string, unknown>>;
    }
  | { kind: "approval_required" };

export interface ChangeRoutingRule {
  /** True for workspace overrides; false for global defaults the
   *  migration seeded. */
  workspace_scoped: boolean;
  /** Snake-case discriminator — `glossary_term_create`, … */
  change_type: string;
  routing: ApprovalRouting;
  risk_level: RiskLevel;
  priority: number;
}

export interface UpsertRoutingRuleRequest {
  routing: ApprovalRouting;
  risk_level?: RiskLevel;
  /** Defaults to 100 server-side so a freshly-edited override
   *  out-ranks the seed default of 0. */
  priority?: number;
}

export async function listRoutingRules(): Promise<ChangeRoutingRule[]> {
  const res = await request<{ data: ChangeRoutingRule[] }>(
    "/admin/governance/routing",
  );
  return res.data;
}

export async function upsertRoutingRule(
  changeType: string,
  body: UpsertRoutingRuleRequest,
): Promise<ChangeRoutingRule> {
  const res = await request<{ data: ChangeRoutingRule }>(
    `/admin/governance/routing/${encodeURIComponent(changeType)}`,
    { method: "PUT", body: JSON.stringify(body) },
  );
  return res.data;
}

export async function deleteRoutingRule(changeType: string): Promise<void> {
  await request(`/admin/governance/routing/${encodeURIComponent(changeType)}`, {
    method: "DELETE",
  });
}

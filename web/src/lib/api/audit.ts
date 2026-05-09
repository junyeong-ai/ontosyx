// API client for the workspace-wide PROV-O audit trail.

import { request } from "./client";
import type { AuditFilter, AuditRecordPage } from "@/types/audit";

export async function listAuditRecords(
  filter: AuditFilter,
  cursor?: string,
  limit = 50,
): Promise<AuditRecordPage> {
  const params = new URLSearchParams();
  if (filter.ontology_id) params.set("ontology_id", filter.ontology_id);
  if (filter.activity_kind) params.set("activity_kind", filter.activity_kind);
  if (filter.agent_kind) params.set("agent_kind", filter.agent_kind);
  if (filter.since) params.set("since", filter.since);
  if (filter.until) params.set("until", filter.until);
  if (cursor) params.set("cursor", cursor);
  params.set("limit", String(limit));

  return request<AuditRecordPage>(`/governance/audit?${params.toString()}`);
}

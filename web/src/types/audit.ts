import type { components } from "@/types/api.generated";
import type { ClientPage } from "./pagination";

export type EntityRef = components["schemas"]["EntityRef"];
export type ProvenanceActivityKind =
  components["schemas"]["ProvenanceActivityKind"];
export type AgentRef = components["schemas"]["AgentRef"];
export type ProvenanceDef = components["schemas"]["ProvenanceDef"];
export type AuditRecord = components["schemas"]["AuditRecord"];
export type AuditRecordPage = ClientPage<components["schemas"]["AuditRecordPage"]>;

export interface AuditFilter {
  ontology_id?: string;
  activity_kind?: ProvenanceActivityKind["kind"];
  agent_kind?: AgentRef["kind"];
  since?: string;
  until?: string;
}

export const ACTIVITY_KINDS = [
  "source_scan",
  "function_eval",
  "rule_validate",
  "action_execute",
  "ontology_edit",
  "draft_proposal",
  "cache_refresh",
  "enrichment",
  "import",
  "export",
] as const satisfies ReadonlyArray<ProvenanceActivityKind["kind"]>;

export const AGENT_KINDS = [
  "user",
  "service",
  "llm_model",
  "system",
] as const satisfies ReadonlyArray<AgentRef["kind"]>;

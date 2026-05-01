// PROV-O record shapes mirrored from `crates/ox-ontology/src/provenance.rs`.
// Single source of truth for the audit page + any other reader of
// the workspace's PROV-O stream.

export type EntityRef =
  | { kind: "node_instance"; node_type_id: string; element_id: string }
  | { kind: "edge_instance"; edge_type_id: string; element_id: string }
  | {
      kind: "property_value";
      node_type_id: string;
      element_id: string;
      property_id: string;
    }
  | { kind: "arbitrary"; label: string };

export type ProvenanceActivityKind =
  | { kind: "source_scan"; source_id: string; mapping_id: string }
  | { kind: "function_eval"; function_id: string }
  | {
      kind: "rule_validate";
      rule_id: string;
      outcome: "pass" | "warn" | "fail";
    }
  | {
      kind: "action_execute";
      action_id: string;
      idempotency_key?: string;
    }
  | { kind: "ontology_edit"; command_summary: string }
  | {
      kind: "draft_proposal";
      prompt_name: string;
      prompt_version: string;
      model_id: string;
    }
  | { kind: "cache_refresh"; mapping_id: string }
  | { kind: "enrichment"; enrichment_id: string }
  | { kind: "import"; format: string; source_uri?: string }
  | { kind: "export"; format: string; destination_uri?: string };

export type AgentRef =
  | { kind: "user"; user_id: string }
  | { kind: "service"; service_id: string }
  | { kind: "llm_model"; model_id: string }
  | { kind: "system" };

export interface ProvenanceDef {
  id: string;
  subject: EntityRef;
  activity: ProvenanceActivityKind;
  agent: AgentRef;
  at_time: string;
  used?: EntityRef[];
  derived_from?: EntityRef[];
  ontology_valid_at?: string;
  data_valid_at?: string;
}

/** Audit record returned by `GET /api/governance/audit`. Surfaces
 *  the source-ontology attribution alongside the `ProvenanceDef`
 *  payload. The wire shape from `api.generated` types `provenance`
 *  as `unknown` (the spec proxy uses `serde_json::Value`); this
 *  alias narrows it to the canonical PROV-O shape so the page can
 *  exhaustive-match the discriminated unions. */
import type { components } from "@/types/api.generated";
export type AuditRecord = Omit<
  components["schemas"]["AuditRecord"],
  "provenance"
> & {
  provenance: ProvenanceDef;
};

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

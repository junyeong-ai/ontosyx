// Ambiguity admin API client.

import { request } from "./client";

export type AmbiguityKindValue = "numeric_code" | "opaque_short_code" | "overloaded_name";

export interface AmbiguityContext {
  id: string;
  source_id: string;
  column: { relation: string; column: string };
  kind: { kind: AmbiguityKindValue };
  sample_values?: string[];
  distinct_estimate?: number;
  nullable?: boolean;
  clarification_prompt: string;
  detection_source_hash: string;
  repo_hint?: { suggested_values: string; source_file: string };
  detected_at: string;
}

export type AmbiguityMapping =
  | {
      kind: "value_map";
      entries: Array<{ value: string; display: string; definition?: string }>;
    }
  | { kind: "code_system_ref"; code_system_id: string }
  | { kind: "glossary_ref"; term_id: string };

export interface AmbiguityResolution {
  id: string;
  context_id: string;
  context_source_hash: string;
  mapping: AmbiguityMapping;
  resolved_at: string;
  resolved_by_user_id?: string;
  supersedes?: string;
  revoked_at?: string;
}

export interface AmbiguitySummary {
  context: AmbiguityContext;
  active_resolution: AmbiguityResolution | null;
}

export async function listAmbiguities(): Promise<{ items: AmbiguitySummary[] }> {
  return request(`/ambiguities`);
}

export async function getAmbiguity(
  id: string,
): Promise<{ context: AmbiguityContext; history: AmbiguityResolution[] }> {
  return request(`/ambiguities/${encodeURIComponent(id)}`);
}

export async function resolveAmbiguity(
  id: string,
  mapping: AmbiguityMapping,
): Promise<AmbiguityResolution> {
  return request(`/ambiguities/${encodeURIComponent(id)}/resolve`, {
    method: "POST",
    body: JSON.stringify({ mapping }),
  });
}

export async function revokeAmbiguity(id: string): Promise<{ revoked: boolean }> {
  return request(`/ambiguities/${encodeURIComponent(id)}/revoke`, {
    method: "POST",
  });
}

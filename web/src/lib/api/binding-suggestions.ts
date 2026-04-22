// Phase 4.5 — Property ↔ registry binding surface.
//
// Wraps the two scoring endpoints (Phase 1 backend) plus the
// `OntologyEditOp::BindPropertyTo*` half of `/edits` so the admin UI
// can "suggest → confirm → persist" in a single pipeline. Every
// response carries structured signal detail (canonical / alias /
// description / fuzzy) so the UI renders *why* a candidate ranks
// where it does.

import { request } from "./client";

// ---------------------------------------------------------------------------
// Shared wire shapes
// ---------------------------------------------------------------------------

export type OwnerKind = "node" | "edge";

export type BindingSignal =
  | { kind: "canonical_name" }
  | { kind: "alias"; detail: string }
  | { kind: "description_overlap"; shared_tokens: number; total_tokens: number }
  | { kind: "fuzzy_name"; ratio: number };

export interface BindingPolicyBody {
  min_score?: number;
  max_results?: number;
  weight_exact_name?: number;
  weight_alias_match?: number;
  weight_description_overlap?: number;
  weight_fuzzy_name?: number;
  fuzzy_min_ratio?: number;
  skip_already_bound?: boolean;
}

// ---------------------------------------------------------------------------
// Term → property ranking
// ---------------------------------------------------------------------------

export interface PropertyCandidate {
  owner_kind: OwnerKind;
  owner_type_id: string;
  owner_label: string;
  property_id: string;
  property_name: string;
  score: number;
  signals: BindingSignal[];
}

export interface SuggestBindingsRequest {
  /** Canonical term name (required). */
  term: string;
  aliases?: string[];
  description?: string;
  /** Passed through when the term is already saved — keeps the
   * response pointing at the persisted id rather than a fresh
   * draft id. */
  term_id?: string;
  policy?: BindingPolicyBody;
}

export interface SuggestBindingsResponse {
  ontology_id: string;
  candidates: PropertyCandidate[];
}

export async function suggestGlossaryBindings(
  ontologyId: string,
  body: SuggestBindingsRequest,
): Promise<SuggestBindingsResponse> {
  return request(
    `/ontologies/${encodeURIComponent(ontologyId)}/glossary/suggest-bindings`,
    { method: "POST", body: JSON.stringify(body) },
  );
}

// ---------------------------------------------------------------------------
// Property → term ranking (inverse direction)
// ---------------------------------------------------------------------------

export interface TermCandidate {
  term_id: string;
  term: string;
  score: number;
  signals: BindingSignal[];
}

export interface SuggestTermsResponse {
  ontology_id: string;
  candidates: TermCandidate[];
}

export async function suggestTermsForProperty(
  ontologyId: string,
  ownerKind: OwnerKind,
  ownerTypeId: string,
  propertyId: string,
  policy?: BindingPolicyBody,
): Promise<SuggestTermsResponse> {
  const path =
    `/ontologies/${encodeURIComponent(ontologyId)}` +
    `/properties/${encodeURIComponent(ownerKind)}` +
    `/${encodeURIComponent(ownerTypeId)}` +
    `/${encodeURIComponent(propertyId)}/suggest-terms`;
  return request(path, {
    method: "POST",
    body: JSON.stringify({ policy }),
  });
}

// ---------------------------------------------------------------------------
// /edits — apply the binding
//
// Keep the edit-op payload tight: the backend re-validates
// referential integrity, so we only send what the variant names.
// ---------------------------------------------------------------------------

export type PropertyOwnerPath =
  | { kind: "node"; type_id: string }
  | { kind: "edge"; type_id: string };

export type BindingEditOp =
  | {
      op: "bind_property_to_term";
      owner: PropertyOwnerPath;
      property_id: string;
      glossary_term_id: string | null;
    }
  | {
      op: "bind_property_to_value_set";
      owner: PropertyOwnerPath;
      property_id: string;
      value_set_id: string | null;
    }
  | {
      op: "bind_property_to_notation_pattern";
      owner: PropertyOwnerPath;
      property_id: string;
      notation_pattern_id: string | null;
    }
  | {
      op: "deprecate_node_type";
      id: string;
      replaced_by_id?: string | null;
    }
  | {
      op: "deprecate_edge_type";
      id: string;
      replaced_by_id?: string | null;
    };

export interface OntologyEditRequest {
  expected_version: number;
  operations: BindingEditOp[];
  message?: string;
  dry_run?: boolean;
}

export interface OntologyEditReceipt {
  new_version: number;
  new_version_id: string;
  parent_version_id: string | null;
  applied_operations: number;
  committed_at: string;
}

export async function applyOntologyEdits(
  ontologyId: string,
  body: OntologyEditRequest,
): Promise<OntologyEditReceipt> {
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/edits`, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

// Property ↔ registry binding surface. Wraps the two scoring
// endpoints + the `OntologyEditOp::BindPropertyTo*` half of /edits
// so the UI can suggest → confirm → persist in one pipeline. Each
// candidate carries structured signal detail (canonical / alias /
// description / fuzzy) so callers can render the ranking rationale.

import { request } from "./client";
import type { LocalizedText } from "@/types/ontology";

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
  /** Canonical term name. `LocalizedText` so a draft term carries
   *  the same multi-locale shape as a saved one — the scorer matches
   *  against every locale variant, not just the active chain's
   *  display string. */
  term: LocalizedText;
  /** Per-locale alternate names. Each element is a `LocalizedText`
   *  with one or more locale entries, all of which the alias scorer
   *  treats as candidate matches. */
  aliases?: readonly LocalizedText[];
  description?: LocalizedText;
  /** Passed through when the term is already saved — keeps the
   *  response pointing at the persisted id rather than a fresh
   *  draft id. */
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
// `BindingEditOp` is the narrow subset of `OntologyEditOp` the
// binding-suggestions UI ever constructs. It locks the form's
// authoring surface at the type level so the binding affordance
// can't accidentally emit, say, a `create_glossary_term` op — the
// full 24-variant union would silently allow that.
// ---------------------------------------------------------------------------

import type {
  PropertyBinding,
  PropertyBindingHandle,
} from "@/types/ontology";
import type { PropertyOwnerPath } from "./edit-ops";

export type BindingEditOp =
  | {
      op: "bind_property";
      owner: PropertyOwnerPath;
      property_id: string;
      binding: PropertyBinding;
    }
  | {
      op: "unbind_property";
      owner: PropertyOwnerPath;
      property_id: string;
      target: PropertyBindingHandle;
    }
  | {
      op: "deprecate_node_type";
      id: string;
      replaced_by_id?: string;
    }
  | {
      op: "deprecate_edge_type";
      id: string;
      replaced_by_id?: string;
    };

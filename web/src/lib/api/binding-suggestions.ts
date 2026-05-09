// Property ↔ registry binding surface. Wraps the two scoring
// endpoints + the `OntologyEditOp::BindPropertyTo*` half of /edits
// so the UI can suggest → confirm → persist in one pipeline. Each
// candidate carries structured signal detail (canonical / alias /
// description / fuzzy) so callers can render the ranking rationale.

import { request } from "./client";
import type { components } from "@/types/api.generated";

// ---------------------------------------------------------------------------
// Shared wire shapes
// ---------------------------------------------------------------------------

export type OwnerKind = "node" | "edge";

export type BindingSignal = components["schemas"]["BindingSignal"];

export type BindingPolicy = components["schemas"]["BindingPolicy"];
export type PropertyCandidate = components["schemas"]["PropertyCandidate"] & {
  owner_kind: OwnerKind;
};
export type ConceptCandidate = components["schemas"]["ConceptCandidate"];

// ---------------------------------------------------------------------------
// Concept label → property ranking
// ---------------------------------------------------------------------------

export type SuggestBindingsRequest = components["schemas"]["SuggestBindingsRequest"];
export type SuggestBindingsResponse = Omit<
  components["schemas"]["SuggestBindingsResponse"],
  "candidates"
> & {
  candidates: PropertyCandidate[];
};

export async function suggestConceptPropertyBindings(
  _ontologyId: string,
  body: SuggestBindingsRequest,
): Promise<SuggestBindingsResponse> {
  return request("/ontology/concepts/suggest-property-bindings", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

// ---------------------------------------------------------------------------
// Property → concept ranking (inverse direction)
// ---------------------------------------------------------------------------

export type SuggestConceptsResponse = components["schemas"]["SuggestConceptsResponse"];

export async function suggestConceptsForProperty(
  _ontologyId: string,
  ownerKind: OwnerKind,
  ownerTypeId: string,
  propertyId: string,
  policy?: BindingPolicy,
): Promise<SuggestConceptsResponse> {
  const path =
    `/ontology/properties/${encodeURIComponent(ownerKind)}` +
    `/${encodeURIComponent(ownerTypeId)}` +
    `/${encodeURIComponent(propertyId)}/suggest-concepts`;
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

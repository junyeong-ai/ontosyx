// Discriminated-union mirror of `ox_ontology::OntologyEditOp`.
//
// CRUD callers construct ops from this union and submit them via
// `submitOntologyEdits` (POST `/api/ontology/edits`). Wire
// format: `serde(tag = "op", rename_all = "snake_case")` →
// `{"op": "create_glossary_term", "def": {...}}`.
//
// Source of truth: `crates/ox-ontology/src/edit.rs::OntologyEditOp`.
// Adding a new variant on the backend requires a parallel entry here
// — there is no codegen step yet, the pair stays hand-mirrored.

import type {
  ConceptMapDef,
  PropertyBinding,
  PropertyBindingHandle,
} from "@/types/ontology";
import type { components } from "@/types/api.generated";
import { request } from "./client";

// ---------------------------------------------------------------------------
// Domain types — most edit-op payloads pass through as open `Record`
// shapes; the backend re-validates referential integrity through
// `OntologyIR::validate()` after the batch, so a wrong shape comes
// back as a structured 422 rather than silently committing.
//
// Glossary edit ops use the canonical OpenAPI-generated types so
// the admin form stays aligned with the wire shape end-to-end —
// lifecycle / governance / examples / per-locale labels all flow
// without a parallel hand-rolled schema.
// ---------------------------------------------------------------------------

export type TermRelationKind =
  components["schemas"]["TermRelationKind"];
export type TermRelation = components["schemas"]["TermRelation"];
export type TermLifecycle = components["schemas"]["TermLifecycle"];
export type TermGovernance = components["schemas"]["TermGovernance"];
export type TermOrigin = components["schemas"]["TermOrigin"];
export type TermChangeNote = components["schemas"]["TermChangeNote"];
export type GlossaryTermDef = components["schemas"]["GlossaryTermDef"];
export type ConceptDef = components["schemas"]["ConceptDef"];

export type CodeSystemKind = components["schemas"]["CodeSystemKind"];
export type CodedValue = components["schemas"]["CodedValue"];
export type CodeSystemDef = components["schemas"]["CodeSystemDef"];
export type ValueSetDef = components["schemas"]["ValueSetDef"];
export type NotationPatternDef = components["schemas"]["NotationPatternDef"];

export type Severity = components["schemas"]["Severity"];
export type EnforcementKind = components["schemas"]["EnforcementKind"];
export type RuleActivationKind = components["schemas"]["RuleActivationKind"];
export type RuleKind = components["schemas"]["RuleKind"];
export type RuleOrigin = components["schemas"]["RuleOrigin"];
export type ConstraintTarget = components["schemas"]["ConstraintTarget"];
export type ShaclConstraint = components["schemas"]["ShaclConstraint"];
export type PropertyType = components["schemas"]["PropertyType"];
export type RuleDef = components["schemas"]["RuleDef"];

export type ObjectMappingDef = components["schemas"]["ObjectMappingDef"];
export type LinkMappingDef = components["schemas"]["LinkMappingDef"];
export type LinkMappingKind = components["schemas"]["LinkMappingKind"];
export type PropertyMappingDef = components["schemas"]["PropertyMappingDef"];
export type PropertyLocation = components["schemas"]["PropertyLocation"];
export type PropertyTransform = components["schemas"]["PropertyTransform"];
export type ColumnRef = components["schemas"]["ColumnRef"];
export type EndpointRef = components["schemas"]["EndpointRef"];
export type SourceRelationKind = components["schemas"]["SourceRelationKind"];
export type CacheHintKind = components["schemas"]["CacheHintKind"];

export type PropertyOwnerPath =
  | { kind: "node"; type_id: string }
  | { kind: "edge"; type_id: string };

// ---------------------------------------------------------------------------
// OntologyEditOp — the full discriminated union.
//
// Grouped by entity kind so the eye finds the variant for a given
// CRUD action quickly. Wire-format snake_case names match
// `OntologyEditOp::serde(rename_all = "snake_case")`.
// ---------------------------------------------------------------------------

export type OntologyEditOp =
  // CodeSystem
  | { op: "create_code_system"; def: CodeSystemDef }
  | { op: "update_code_system"; id: string; def: CodeSystemDef }
  | { op: "delete_code_system"; id: string }
  // CodedValue (nested under CodeSystem)
  | { op: "create_coded_value"; code_system_id: string; value: CodedValue }
  | {
      op: "update_coded_value";
      code_system_id: string;
      id: string;
      value: CodedValue;
    }
  | { op: "delete_coded_value"; code_system_id: string; id: string }
  // Concept
  | { op: "create_concept"; def: ConceptDef }
  | { op: "update_concept"; id: string; def: ConceptDef }
  | { op: "delete_concept"; id: string }
  // GlossaryTerm
  | { op: "create_glossary_term"; def: GlossaryTermDef }
  | { op: "update_glossary_term"; id: string; def: GlossaryTermDef }
  | { op: "delete_glossary_term"; id: string }
  // ObjectMapping
  | { op: "create_object_mapping"; mapping: ObjectMappingDef }
  | { op: "update_object_mapping"; id: string; mapping: ObjectMappingDef }
  | { op: "delete_object_mapping"; id: string }
  // LinkMapping
  | { op: "create_link_mapping"; mapping: LinkMappingDef }
  | { op: "update_link_mapping"; id: string; mapping: LinkMappingDef }
  | { op: "delete_link_mapping"; id: string }
  // NotationPattern
  | { op: "create_notation_pattern"; def: NotationPatternDef }
  | { op: "update_notation_pattern"; id: string; def: NotationPatternDef }
  | { op: "delete_notation_pattern"; id: string }
  // ConceptMap
  | { op: "create_concept_map"; def: ConceptMapDef }
  | { op: "update_concept_map"; id: string; def: ConceptMapDef }
  | { op: "delete_concept_map"; id: string }
  // ValueSet
  | { op: "create_value_set"; def: ValueSetDef }
  | { op: "update_value_set"; id: string; def: ValueSetDef }
  | { op: "delete_value_set"; id: string }
  // Rule
  | { op: "create_rule"; def: RuleDef }
  | { op: "update_rule"; id: string; def: RuleDef }
  | { op: "delete_rule"; id: string }
  // Type deprecation
  | { op: "deprecate_node_type"; id: string; replaced_by_id?: string }
  | { op: "deprecate_edge_type"; id: string; replaced_by_id?: string }
  // Property → registry bindings
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
    };

// ---------------------------------------------------------------------------
// Request / response wrappers.
// ---------------------------------------------------------------------------

export type EditOntologyRequest = Omit<
  components["schemas"]["EditOntologyRequest"],
  "operations"
> & {
  operations: OntologyEditOp[];
};
export type OntologyEditReceipt = components["schemas"]["OntologyEditReceipt"];
export type OntologyEditPreCheck = components["schemas"]["OntologyEditPreCheck"];
export type EditOntologyResponse = components["schemas"]["EditOntologyResponse"];

export function isOntologyEditReceipt(
  response: EditOntologyResponse,
): response is OntologyEditReceipt {
  return "new_version_id" in response;
}

/** Submit a batch of edit operations against an ontology. The
 *  server validates each op + the post-batch IR; on success returns
 *  the new committed version. */
export async function submitOntologyEdits(
  _ontologyId: string,
  body: EditOntologyRequest,
): Promise<EditOntologyResponse> {
  return request("/ontology/edits", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

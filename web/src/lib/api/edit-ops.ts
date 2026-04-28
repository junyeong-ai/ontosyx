// Discriminated-union mirror of `ox_ontology::OntologyEditOp`.
//
// Every Φ4 vocabulary CRUD page constructs ops from this union and
// submits them through `applyOntologyEdits` (POST
// `/api/ontologies/{id}/edits`). The wire format uses
// `serde(tag = "op", rename_all = "snake_case")` on the Rust side, so
// the JSON shape is `{"op": "create_glossary_term", "def": {...}}`.
//
// Source of truth: `crates/ox-ontology/src/edit.rs::OntologyEditOp`.
// Adding a new variant on the backend requires a parallel entry here
// — there is no codegen step yet, the pair stays hand-mirrored.

import type {
  ConceptMapDef,
  LocalizedText,
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

export type CodeSystemKind = "international" | "standard" | "internal" | "custom";

export type CodedValue = {
  id: string;
  code: string;
  display?: LocalizedText;
  definition?: LocalizedText;
  aliases?: string[];
  broader_id?: string | null;
  examples?: LocalizedText[];
  scope_note?: LocalizedText;
  valid_from?: string | null;
  valid_to?: string | null;
  deprecated_at?: string | null;
  replaced_by_id?: string | null;
};

export type CodeSystemDef = {
  id: string;
  name: string;
  display_name?: LocalizedText;
  description?: LocalizedText;
  version: string;
  kind: CodeSystemKind;
  uri?: string | null;
  hierarchical?: boolean;
  deprecated_at?: string | null;
  replaced_by_id?: string | null;
  codes: CodedValue[];
};

export type ValueSetDef = {
  id: string;
  name: string;
  display_name?: LocalizedText;
  description?: LocalizedText;
  version: string;
  composition?: Array<{
    system_id: string;
    selector: { kind: "all" } | { kind: "explicit"; codes: string[] };
    mode: "include" | "exclude";
  }>;
};

export type NotationPatternDef = {
  id: string;
  name: string;
  display_name?: LocalizedText;
  description?: LocalizedText;
  template: string;
  separator?: string;
  components: Array<Record<string, unknown>>;
  examples?: string[];
};

// ---------------------------------------------------------------------------
// Rule wire shapes — mirror the OpenAPI-generated `components["schemas"]`
// projection of `ox_ontology::rule`. Hand-rolled here (rather than aliased
// from `api.generated`) so the edit-ops surface stays self-contained for the
// admin CRUD pages: every editor reads + writes the same `RuleDef` shape.
// ---------------------------------------------------------------------------

export type Severity = "violation" | "warning" | "info";
export type EnforcementKind = "write" | "read" | "batch";

export type RuleActivationKind =
  | { kind: "always" }
  | { kind: "on_action"; action_id: string }
  | { kind: "on_schedule"; cron_expression: string };

export type RuleKind =
  | { kind: "node_shape"; target_node_type_id: string }
  | {
      kind: "property_shape";
      target_node_type_id: string;
      target_property_id: string;
    }
  | { kind: "edge_shape"; target_edge_type_id: string }
  | { kind: "cross_entity_shape"; predicate: string }
  | {
      kind: "state_machine";
      target_node_type_id: string;
      state_property_id: string;
      transitions: Array<{ from?: string | null; to: string }>;
    };

export type RuleOrigin =
  | { kind: "authored" }
  | {
      kind: "derived_from_binding";
      node_type_id: string;
      property_id: string;
    };

export type ConstraintTarget =
  | { kind: "inherit" }
  | {
      kind: "property";
      node_type_id: string;
      property_id: string;
    }
  | { kind: "node_type"; node_type_id: string }
  | { kind: "edge_label"; edge_label: string };

/**
 * SHACL constraint variants — AND'd together inside a single
 * [`RuleDef.constraints`] list. The editor forms one variant at a time
 * via the constraint-kind-pluggable form registry.
 */
export type ShaclConstraint =
  | { kind: "min_count"; target: ConstraintTarget; min: number }
  | { kind: "max_count"; target: ConstraintTarget; max: number }
  | { kind: "datatype"; target: ConstraintTarget; expected: string }
  | {
      kind: "matches_pattern";
      target: ConstraintTarget;
      notation_pattern_id: string;
    }
  | {
      kind: "in_value_set";
      target: ConstraintTarget;
      value_set_id: string;
    }
  | { kind: "has_value"; target: ConstraintTarget; value: string }
  | { kind: "min_inclusive"; target: ConstraintTarget; min: number }
  | { kind: "max_inclusive"; target: ConstraintTarget; max: number }
  | { kind: "min_length"; target: ConstraintTarget; min: number }
  | { kind: "max_length"; target: ConstraintTarget; max: number }
  | { kind: "unique_lang"; target: ConstraintTarget }
  | {
      kind: "closed";
      target: ConstraintTarget;
      allowed_properties: string[];
    }
  | { kind: "disjoint"; a: ConstraintTarget; b: ConstraintTarget }
  | {
      kind: "unique_key";
      target_node_type_id: string;
      property_keys: string[];
    };

export type RuleDef = {
  id: string;
  name: LocalizedText;
  description?: LocalizedText;
  rationale?: LocalizedText;
  kind: RuleKind;
  severity?: Severity;
  enforcement?: EnforcementKind;
  activation?: RuleActivationKind;
  origin?: RuleOrigin;
  constraints?: ShaclConstraint[];
  valid_from?: string | null;
  valid_to?: string | null;
};

export type ObjectMappingDef = components["schemas"]["ObjectMappingDef"];
export type LinkMappingDef = components["schemas"]["LinkMappingDef"];
export type PropertyMappingDef = components["schemas"]["PropertyMappingDef"];
export type PropertyLocation = components["schemas"]["PropertyLocation"];
export type PropertyTransform = components["schemas"]["PropertyTransform"];
export type ColumnRef = components["schemas"]["ColumnRef"];
export type SourceRelationKind = components["schemas"]["SourceRelationKind"];
export type CacheHintKind = components["schemas"]["CacheHintKind"];

export type PropertyOwnerPath =
  | { kind: "node"; type_id: string }
  | { kind: "edge"; type_id: string };

// ---------------------------------------------------------------------------
// OntologyEditOp — the full discriminated union (24 variants).
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
  | { op: "deprecate_node_type"; id: string; replaced_by_id?: string | null }
  | { op: "deprecate_edge_type"; id: string; replaced_by_id?: string | null }
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

export interface EditOntologyRequest {
  expected_version: number;
  operations: OntologyEditOp[];
  message?: string;
  /** Pre-check without committing. Server runs the apply + validate
   *  pass against a clone and returns the diagnostics; nothing
   *  persists. Useful for "would this break the IR?" form previews. */
  dry_run?: boolean;
}

export interface OntologyEditReceipt {
  new_version: number;
  new_version_id: string;
  parent_version_id: string | null;
  applied_operations: number;
  committed_at: string;
}

/** Submit a batch of edit operations against an ontology. The
 *  server validates each op + the post-batch IR; on success returns
 *  the new committed version. */
export async function submitOntologyEdits(
  ontologyId: string,
  body: EditOntologyRequest,
): Promise<OntologyEditReceipt> {
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/edits`, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

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

import type { LocalizedText } from "@/types/ontology";
import { request } from "./client";

// ---------------------------------------------------------------------------
// Domain types — minimal pass-through shapes the edit ops carry.
//
// We intentionally leave most of these typed as the open `Record`
// shape rather than re-declaring every IR Def in TypeScript. The
// backend re-validates referential integrity through `OntologyIR
// ::validate()` after applying the batch, so a wrong shape produces
// a 422 with the structured error — typing each field on the
// front-end would amount to maintaining a parallel schema (see
// `tools/openapi-codegen` for the long-term direction).
// ---------------------------------------------------------------------------

export type GlossaryTermDef = {
  id: string;
  term: string;
  display_name?: LocalizedText;
  description?: LocalizedText;
  category?: string | null;
  aliases?: string[];
  parent_term_id?: string | null;
};

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

export type ConceptMapDef = {
  id: string;
  name: string;
  display_name?: LocalizedText;
  description?: LocalizedText;
  version: string;
  source_system_id: string;
  target_system_id: string;
  mappings: Array<{
    source_code: string;
    target_code: string;
    equivalence:
      | "equivalent"
      | "narrower_than_target"
      | "broader_than_target"
      | "related"
      | "not_related";
    comment?: LocalizedText;
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

export type RuleDef = {
  id: string;
  name: string;
  description?: LocalizedText;
  kind: Record<string, unknown>;
  constraints?: Array<Record<string, unknown>>;
  enforcement?: "write_time" | "read_time" | "batch";
  severity?: "info" | "warn" | "fail";
  active?: boolean;
};

export type ObjectMappingDef = Record<string, unknown>;
export type LinkMappingDef = Record<string, unknown>;

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
    };

// ---------------------------------------------------------------------------
// Request / response wrappers.
// ---------------------------------------------------------------------------

export interface OntologyEditRequest {
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
  body: OntologyEditRequest,
): Promise<OntologyEditReceipt> {
  return request(`/ontologies/${encodeURIComponent(ontologyId)}/edits`, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

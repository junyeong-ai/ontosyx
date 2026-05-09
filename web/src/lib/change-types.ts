import type { components } from "@/types/api.generated";

export type ChangeType = components["schemas"]["ChangeType"];

export const CHANGE_TYPE_ORDER = [
  "coded_value_create",
  "coded_value_deprecate",
  "terminology_registry_update",
  "semantic_binding_update",
  "notation_pattern_create",
  "customer_segment_create",
  "column_rename",
  "table_merge",
  "data_source_register",
  "stale_concept_deprecate",
  "ontology_version_rollback",
  "rule_create",
  "rule_modify",
  "rule_delete",
] as const satisfies readonly ChangeType[];

type MissingChangeType = Exclude<ChangeType, (typeof CHANGE_TYPE_ORDER)[number]>;

export const CHANGE_TYPE_ORDER_IS_EXHAUSTIVE: MissingChangeType extends never
  ? true
  : never = true;

export const CHANGE_TYPE_RANK: ReadonlyMap<string, number> = new Map(
  CHANGE_TYPE_ORDER.map((changeType, index) => [changeType, index]),
);

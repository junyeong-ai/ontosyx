import { useTranslations } from "next-intl";

import type {
  ColumnRef,
  EndpointRef,
  LinkMappingDef,
  LinkMappingKind,
  SourceRelationKind,
} from "@/lib/api/edit-ops";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import type { components } from "@/types/api.generated";

// LinkMappingDef entity schema. The wire shape carries a discriminated
// `kind` (foreign_key / bridge / computed / federated) — each variant
// has its own typed sub-schema, recursed through the generic
// `discriminated` field renderer. ColumnRef / SourceRelationRef /
// EndpointRef nest as typed compound values via the `nested` field
// kind.

type SourceRelationRef = components["schemas"]["SourceRelationRef"];

// ---------------------------------------------------------------------------
// Reference sub-schemas — typed compound values shared across variants
// ---------------------------------------------------------------------------

export const columnRefSchema: EntitySchema<ColumnRef> = {
  entityKind: "mappings.form.columnRef",
  buildDefault: () => ({ relation: "", column: "" }),
  fields: [
    {
      kind: "text",
      key: "relation",
      labelKey: "relation",
      required: true,
      placeholder: "public.customers",
      monospace: true,
    },
    {
      kind: "text",
      key: "column",
      labelKey: "column",
      required: true,
      placeholder: "id",
      monospace: true,
    },
  ],
};

const SOURCE_RELATION_KIND_OPTIONS: ReadonlyArray<{
  value: SourceRelationKind;
  labelKey: string;
}> = [
  { value: "table", labelKey: "kindOptions.table" },
  { value: "view", labelKey: "kindOptions.view" },
  { value: "collection", labelKey: "kindOptions.collection" },
  { value: "file", labelKey: "kindOptions.file" },
];

export const sourceRelationRefSchema: EntitySchema<SourceRelationRef> = {
  entityKind: "mappings.form.sourceRelationRef",
  buildDefault: () => ({
    source_id: "",
    relation: "",
    kind: "table",
  }),
  fields: [
    {
      kind: "text",
      key: "source_id",
      labelKey: "sourceId",
      required: true,
      placeholder: "src-postgres",
      monospace: true,
    },
    {
      kind: "text",
      key: "relation",
      labelKey: "relation",
      required: true,
      placeholder: "public.customer_orders",
      monospace: true,
    },
    {
      kind: "enum",
      key: "kind",
      labelKey: "kind",
      options: SOURCE_RELATION_KIND_OPTIONS,
    },
  ],
};

// EndpointRef.key_columns is a `string[]` — a primitive-array shape
// that doesn't fit `ListField` (which carries an item EntitySchema).
// Inlined as a `custom` field — an opt-in escape hatch the schema
// catalogue documents for exactly this shape.
function StringListInput({
  value,
  onChange,
  disabled,
  placeholder,
  addLabel,
  removeAriaLabel,
}: {
  value: string[];
  onChange: (next: string[]) => void;
  disabled?: boolean;
  placeholder?: string;
  addLabel: string;
  removeAriaLabel: string;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      {value.length === 0 ? null : (
        <ul className="flex flex-col gap-1">
          {value.map((entry, idx) => (
            // List index is the only stable handle for a primitive
            // array; entries can repeat across rows.
            // eslint-disable-next-line react/no-array-index-key
            <li key={idx} className="flex items-center gap-1.5">
              <FormInput
                value={entry}
                onChange={(e) => {
                  const next = value.slice();
                  next[idx] = e.target.value;
                  onChange(next);
                }}
                placeholder={placeholder}
                density="compact"
                disabled={disabled}
                className="font-mono"
              />
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onChange(value.filter((_, i) => i !== idx))}
                disabled={disabled}
                aria-label={removeAriaLabel}
              >
                ✕
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => onChange([...value, ""])}
          disabled={disabled}
        >
          {addLabel}
        </Button>
      </div>
    </div>
  );
}

function KeyColumnsControl({
  value,
  onChange,
  disabled,
}: {
  value: unknown;
  onChange: (next: unknown) => void;
  disabled?: boolean;
}) {
  const t = useTranslations(
    "mappings.form.endpointRef",
  );
  const items = Array.isArray(value) ? (value as string[]) : [];
  return (
    <StringListInput
      value={items}
      onChange={onChange}
      disabled={disabled}
      placeholder={t("keyColumnPlaceholder")}
      addLabel={t("keyColumnsAdd")}
      removeAriaLabel={t("keyColumnsRemove")}
    />
  );
}

export const endpointRefSchema: EntitySchema<EndpointRef> = {
  entityKind: "mappings.form.endpointRef",
  buildDefault: () => ({
    source_id: "",
    relation: "",
    key_columns: [],
  }),
  fields: [
    {
      kind: "text",
      key: "source_id",
      labelKey: "sourceId",
      required: true,
      placeholder: "src-postgres",
      monospace: true,
    },
    {
      kind: "text",
      key: "relation",
      labelKey: "relation",
      required: true,
      placeholder: "public.customers",
      monospace: true,
    },
    {
      kind: "custom",
      key: "key_columns",
      labelKey: "keyColumns",
      descriptionKey: "keyColumnsDescription",
      render: ({ value, onChange }) => (
        <KeyColumnsControl value={value} onChange={onChange} />
      ),
    },
  ],
};

// ---------------------------------------------------------------------------
// Variant sub-schemas — one per LinkMappingKind branch
// ---------------------------------------------------------------------------

type ForeignKeyVariant = Extract<LinkMappingKind, { kind: "foreign_key" }>;
type BridgeVariant = Extract<LinkMappingKind, { kind: "bridge" }>;
type ComputedVariant = Extract<LinkMappingKind, { kind: "computed" }>;
type FederatedVariant = Extract<LinkMappingKind, { kind: "federated" }>;

const columnRefAsUnknown = columnRefSchema as EntitySchema<unknown>;
const sourceRelationRefAsUnknown =
  sourceRelationRefSchema as EntitySchema<unknown>;

const foreignKeyVariantSchema: EntitySchema<ForeignKeyVariant> = {
  entityKind: "mappings.form.linkKind.foreignKey",
  buildDefault: () => ({
    kind: "foreign_key",
    source_column: columnRefSchema.buildDefault(),
    target_column: columnRefSchema.buildDefault(),
  }),
  fields: [
    {
      kind: "nested",
      key: "source_column",
      labelKey: "sourceColumn",
      schema: columnRefAsUnknown,
    },
    {
      kind: "nested",
      key: "target_column",
      labelKey: "targetColumn",
      schema: columnRefAsUnknown,
    },
  ],
};

const bridgeVariantSchema: EntitySchema<BridgeVariant> = {
  entityKind: "mappings.form.linkKind.bridge",
  buildDefault: () => ({
    kind: "bridge",
    bridge_relation: sourceRelationRefSchema.buildDefault(),
    source_join: [columnRefSchema.buildDefault()],
    target_join: [columnRefSchema.buildDefault()],
  }),
  fields: [
    {
      kind: "nested",
      key: "bridge_relation",
      labelKey: "bridgeRelation",
      schema: sourceRelationRefAsUnknown,
    },
    {
      kind: "list",
      key: "source_join",
      labelKey: "sourceJoin",
      addLabelKey: "sourceJoinAdd",
      emptyTitleKey: "sourceJoinEmptyTitle",
      emptyDescriptionKey: "sourceJoinEmptyDescription",
      itemSchema: columnRefAsUnknown,
      newItem: () => columnRefSchema.buildDefault(),
      itemKey: (item, idx) => {
        const c = item as ColumnRef;
        return `${c.relation}.${c.column}-${idx}`;
      },
      rowPreview: (item) => {
        const c = item as ColumnRef;
        return (
          <span className="font-mono text-2xs text-foreground-muted">
            {c.relation || "?"}.{c.column || "?"}
          </span>
        );
      },
    } as FieldSchema<BridgeVariant>,
    {
      kind: "list",
      key: "target_join",
      labelKey: "targetJoin",
      addLabelKey: "targetJoinAdd",
      emptyTitleKey: "targetJoinEmptyTitle",
      emptyDescriptionKey: "targetJoinEmptyDescription",
      itemSchema: columnRefAsUnknown,
      newItem: () => columnRefSchema.buildDefault(),
      itemKey: (item, idx) => {
        const c = item as ColumnRef;
        return `${c.relation}.${c.column}-${idx}`;
      },
      rowPreview: (item) => {
        const c = item as ColumnRef;
        return (
          <span className="font-mono text-2xs text-foreground-muted">
            {c.relation || "?"}.{c.column || "?"}
          </span>
        );
      },
    } as FieldSchema<BridgeVariant>,
    {
      kind: "nested",
      key: "bridge_workspace_scope",
      labelKey: "bridgeWorkspaceScope",
      descriptionKey: "bridgeWorkspaceScopeDescription",
      schema: columnRefAsUnknown,
    },
  ],
};

const computedVariantSchema: EntitySchema<ComputedVariant> = {
  entityKind: "mappings.form.linkKind.computed",
  buildDefault: () => ({
    kind: "computed",
    predicate: "",
  }),
  fields: [
    {
      kind: "text",
      key: "predicate",
      labelKey: "predicate",
      descriptionKey: "predicateDescription",
      required: true,
      multiline: true,
      monospace: true,
      placeholder: "src.user_id = tgt.id AND tgt.tenant = 'cj'",
    },
  ],
};

const federatedVariantSchema: EntitySchema<FederatedVariant> = {
  entityKind: "mappings.form.linkKind.federated",
  buildDefault: () => ({
    kind: "federated",
    source_match_column: columnRefSchema.buildDefault(),
    target_match_column: columnRefSchema.buildDefault(),
  }),
  fields: [
    {
      kind: "nested",
      key: "source_match_column",
      labelKey: "sourceMatchColumn",
      schema: columnRefAsUnknown,
    },
    {
      kind: "nested",
      key: "target_match_column",
      labelKey: "targetMatchColumn",
      schema: columnRefAsUnknown,
    },
  ],
};

const LINK_KIND_VARIANTS: Readonly<Record<string, EntitySchema<unknown>>> = {
  foreign_key: foreignKeyVariantSchema as EntitySchema<unknown>,
  bridge: bridgeVariantSchema as EntitySchema<unknown>,
  computed: computedVariantSchema as EntitySchema<unknown>,
  federated: federatedVariantSchema as EntitySchema<unknown>,
};

// ---------------------------------------------------------------------------
// Top-level LinkMappingDef schema
// ---------------------------------------------------------------------------

const JOIN_COST_HINT_OPTIONS = [
  { value: "unknown", labelKey: "joinCostHintOptions.unknown" },
  { value: "indexed", labelKey: "joinCostHintOptions.indexed" },
  { value: "scan", labelKey: "joinCostHintOptions.scan" },
  { value: "cartesian", labelKey: "joinCostHintOptions.cartesian" },
] as const;

const CARDINALITY_OPTIONS = [
  { value: "one_to_one", labelKey: "cardinalityOptions.one_to_one" },
  { value: "one_to_many", labelKey: "cardinalityOptions.one_to_many" },
  { value: "many_to_one", labelKey: "cardinalityOptions.many_to_one" },
  { value: "many_to_many", labelKey: "cardinalityOptions.many_to_many" },
] as const;

const endpointRefAsUnknown = endpointRefSchema as EntitySchema<unknown>;

export const linkMappingSchema: EntitySchema<LinkMappingDef> = {
  entityKind: "mappings.form.link",
  buildDefault: () => ({
    id: "",
    edge_type_id: "",
    kind: foreignKeyVariantSchema.buildDefault(),
    source_endpoint: endpointRefSchema.buildDefault(),
    target_endpoint: endpointRefSchema.buildDefault(),
    join_cost_hint: "unknown",
    precedence: 0,
    cardinality: "many_to_many",
  }),
  validate: (record) => {
    const issues = [];
    if (record.id && !/^[a-z][a-z0-9:_-]*$/i.test(record.id)) {
      issues.push({ messageKey: "idFormat", params: { field: "id" } });
    }
    return issues;
  },
  layout: {
    sections: [
      {
        titleKey: "mappings.form.sections.identity",
        fieldKeys: ["id", "edge_type_id"],
      },
      {
        titleKey: "mappings.form.sections.topology",
        fieldKeys: ["kind"],
      },
      {
        titleKey: "mappings.form.sections.endpoints",
        fieldKeys: ["source_endpoint", "target_endpoint"],
      },
      {
        titleKey: "mappings.form.sections.planner",
        fieldKeys: ["cardinality", "join_cost_hint", "precedence"],
        defaultOpen: false,
      },
    ],
  },
  fields: [
    {
      kind: "text",
      key: "id",
      labelKey: "id",
      required: true,
      monospace: true,
      placeholder: "lm-customer-orders",
      readOnly: (record) =>
        record.id !== "" && record.id !== linkMappingSchema.buildDefault().id,
    },
    {
      kind: "ref",
      key: "edge_type_id",
      labelKey: "edgeTypeId",
      required: true,
      entityKind: "edge_type",
    },
    {
      kind: "discriminated",
      key: "kind",
      labelKey: "kind",
      tag: "kind",
      variants: LINK_KIND_VARIANTS,
    },
    {
      kind: "nested",
      key: "source_endpoint",
      labelKey: "sourceEndpoint",
      descriptionKey: "sourceEndpointDescription",
      schema: endpointRefAsUnknown,
    },
    {
      kind: "nested",
      key: "target_endpoint",
      labelKey: "targetEndpoint",
      descriptionKey: "targetEndpointDescription",
      schema: endpointRefAsUnknown,
    },
    {
      kind: "enum",
      key: "cardinality",
      labelKey: "cardinality",
      descriptionKey: "cardinalityDescription",
      options: CARDINALITY_OPTIONS,
    },
    {
      kind: "enum",
      key: "join_cost_hint",
      labelKey: "joinCostHint",
      options: JOIN_COST_HINT_OPTIONS,
    },
    {
      kind: "number",
      key: "precedence",
      labelKey: "precedence",
      descriptionKey: "precedenceDescription",
      min: 0,
      step: 1,
    },
  ],
};

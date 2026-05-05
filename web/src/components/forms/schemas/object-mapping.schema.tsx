import type {
  CacheHintKind,
  ColumnRef,
  ObjectMappingDef,
  PropertyLocation,
  PropertyMappingDef,
  PropertyTransform,
  SourceRelationKind,
} from "@/lib/api/edit-ops";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";

import { columnRefSchema } from "./link-mapping.schema";

// ObjectMappingDef entity schema. Most fields are typed primitives;
// the load-bearing complexity is the `property_mappings` list and the
// `cache_hint` discriminated value. PropertyLocation and
// PropertyTransform each carry their own discriminated sub-schema —
// the renderer recurses through them automatically.

// ---------------------------------------------------------------------------
// PropertyLocation variants — flat-shape because the Rust enum tags
// `Column(ColumnRef)` as `#[serde(tag = "kind")]` which flattens the
// inner struct's fields next to the tag.
// ---------------------------------------------------------------------------

type ColumnLocationVariant = Extract<PropertyLocation, { kind: "column" }>;
type JsonPathLocationVariant = Extract<PropertyLocation, { kind: "json_path" }>;

const columnLocationVariantSchema: EntitySchema<ColumnLocationVariant> = {
  entityKind: "settings.knowledge.mappings.form.location.column",
  buildDefault: () => ({
    kind: "column",
    relation: "",
    column: "",
  }),
  fields: [
    {
      kind: "text",
      key: "relation",
      labelKey: "relation",
      required: true,
      monospace: true,
      placeholder: "public.customers",
    },
    {
      kind: "text",
      key: "column",
      labelKey: "column",
      required: true,
      monospace: true,
      placeholder: "email",
    },
  ],
};

const jsonPathLocationVariantSchema: EntitySchema<JsonPathLocationVariant> = {
  entityKind: "settings.knowledge.mappings.form.location.jsonPath",
  buildDefault: () => ({
    kind: "json_path",
    root_column: "",
    path: "",
  }),
  fields: [
    {
      kind: "text",
      key: "root_column",
      labelKey: "rootColumn",
      required: true,
      monospace: true,
      placeholder: "metadata",
    },
    {
      kind: "text",
      key: "path",
      labelKey: "path",
      required: true,
      monospace: true,
      placeholder: "address.zip",
    },
  ],
};

const PROPERTY_LOCATION_VARIANTS: Readonly<
  Record<string, EntitySchema<unknown>>
> = {
  column: columnLocationVariantSchema as EntitySchema<unknown>,
  json_path: jsonPathLocationVariantSchema as EntitySchema<unknown>,
};

// ---------------------------------------------------------------------------
// PropertyTransform variants
// ---------------------------------------------------------------------------

type IdentityTransformVariant = Extract<
  PropertyTransform,
  { kind: "identity" }
>;
type SqlExprTransformVariant = Extract<
  PropertyTransform,
  { kind: "sql_expr" }
>;
type DerivedTransformVariant = Extract<
  PropertyTransform,
  { kind: "derived" }
>;

const identityTransformVariantSchema: EntitySchema<IdentityTransformVariant> = {
  entityKind: "settings.knowledge.mappings.form.transform.identity",
  buildDefault: () => ({ kind: "identity" }),
  fields: [],
};

const sqlExprTransformVariantSchema: EntitySchema<SqlExprTransformVariant> = {
  entityKind: "settings.knowledge.mappings.form.transform.sqlExpr",
  buildDefault: () => ({ kind: "sql_expr", expression: "" }),
  fields: [
    {
      kind: "text",
      key: "expression",
      labelKey: "expression",
      descriptionKey: "expressionDescription",
      required: true,
      multiline: true,
      monospace: true,
      placeholder: "lower(email)",
    },
  ],
};

const derivedTransformVariantSchema: EntitySchema<DerivedTransformVariant> = {
  entityKind: "settings.knowledge.mappings.form.transform.derived",
  buildDefault: () => ({ kind: "derived", function_id: "" }),
  fields: [
    {
      kind: "ref",
      key: "function_id",
      labelKey: "functionId",
      required: true,
      entityKind: "function",
    },
  ],
};

const PROPERTY_TRANSFORM_VARIANTS: Readonly<
  Record<string, EntitySchema<unknown>>
> = {
  identity: identityTransformVariantSchema as EntitySchema<unknown>,
  sql_expr: sqlExprTransformVariantSchema as EntitySchema<unknown>,
  derived: derivedTransformVariantSchema as EntitySchema<unknown>,
};

// ---------------------------------------------------------------------------
// PropertyMappingDef — list item schema for ObjectMappingDef.property_mappings
// ---------------------------------------------------------------------------

const propertyMappingItemSchema: EntitySchema<PropertyMappingDef> = {
  entityKind: "settings.knowledge.mappings.form.propertyMapping",
  buildDefault: () => ({
    property_id: "",
    property_key: "",
    location: columnLocationVariantSchema.buildDefault(),
    transform: identityTransformVariantSchema.buildDefault(),
  }),
  fields: [
    {
      kind: "ref",
      key: "property_id",
      labelKey: "propertyId",
      required: true,
      entityKind: "property",
    },
    {
      kind: "text",
      key: "property_key",
      labelKey: "propertyKey",
      required: true,
      monospace: true,
      placeholder: "email",
    },
    {
      kind: "discriminated",
      key: "location",
      labelKey: "location",
      tag: "kind",
      variants: PROPERTY_LOCATION_VARIANTS,
    },
    {
      kind: "discriminated",
      key: "transform",
      labelKey: "transform",
      tag: "kind",
      variants: PROPERTY_TRANSFORM_VARIANTS,
    },
    {
      kind: "ref",
      key: "concept_map_id",
      labelKey: "conceptMapId",
      descriptionKey: "conceptMapIdDescription",
      entityKind: "concept_map",
    },
  ],
};

// ---------------------------------------------------------------------------
// CacheHintKind discriminated value
// ---------------------------------------------------------------------------

type CacheNoneVariant = Extract<CacheHintKind, { kind: "none" }>;
type GraphCacheVariant = Extract<CacheHintKind, { kind: "graph_cache" }>;

const cacheNoneVariantSchema: EntitySchema<CacheNoneVariant> = {
  entityKind: "settings.knowledge.mappings.form.cacheHint.none",
  buildDefault: () => ({ kind: "none" }),
  fields: [],
};

const graphCacheVariantSchema: EntitySchema<GraphCacheVariant> = {
  entityKind: "settings.knowledge.mappings.form.cacheHint.graphCache",
  buildDefault: () => ({
    kind: "graph_cache",
    ttl_seconds: 3600,
    schedule: "",
  }),
  fields: [
    {
      kind: "number",
      key: "ttl_seconds",
      labelKey: "ttlSeconds",
      descriptionKey: "ttlSecondsDescription",
      min: 1,
      step: 60,
    },
    {
      kind: "text",
      key: "schedule",
      labelKey: "schedule",
      descriptionKey: "scheduleDescription",
      placeholder: "0 */6 * * *",
      monospace: true,
    },
  ],
};

const CACHE_HINT_VARIANTS: Readonly<
  Record<string, EntitySchema<unknown>>
> = {
  none: cacheNoneVariantSchema as EntitySchema<unknown>,
  graph_cache: graphCacheVariantSchema as EntitySchema<unknown>,
};

// ---------------------------------------------------------------------------
// Top-level ObjectMappingDef schema
// ---------------------------------------------------------------------------

const SOURCE_RELATION_KIND_OPTIONS: ReadonlyArray<{
  value: SourceRelationKind;
  labelKey: string;
}> = [
  { value: "table", labelKey: "relationKindOptions.table" },
  { value: "view", labelKey: "relationKindOptions.view" },
  { value: "collection", labelKey: "relationKindOptions.collection" },
  { value: "file", labelKey: "relationKindOptions.file" },
];

const columnRefAsUnknown = columnRefSchema as EntitySchema<unknown>;

const columnRefListField = (
  key: keyof ObjectMappingDef & string,
  labelKey: string,
  descriptionKey: string,
): FieldSchema<ObjectMappingDef> =>
  ({
    kind: "list",
    key,
    labelKey,
    descriptionKey,
    addLabelKey: `${labelKey}Add`,
    emptyTitleKey: `${labelKey}EmptyTitle`,
    emptyDescriptionKey: `${labelKey}EmptyDescription`,
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
  }) as FieldSchema<ObjectMappingDef>;

export const objectMappingSchema: EntitySchema<ObjectMappingDef> = {
  entityKind: "settings.knowledge.mappings.form.object",
  buildDefault: () => ({
    id: "",
    node_type_id: "",
    source_id: "",
    relation: "",
    relation_kind: "table",
    primary_key_columns: [],
    partition_columns: [],
    property_mappings: [],
    precedence: 0,
    cache_hint: cacheNoneVariantSchema.buildDefault(),
  }),
  validate: (record) => {
    const issues = [];
    if (record.id && !/^[a-z][a-z0-9:_-]*$/i.test(record.id)) {
      issues.push({ messageKey: "idFormat", params: { field: "id" } });
    }
    if (record.valid_from && record.valid_to) {
      if (new Date(record.valid_to) <= new Date(record.valid_from)) {
        issues.push({
          messageKey: "validityWindow",
          params: { field: "valid_to" },
        });
      }
    }
    return issues;
  },
  layout: {
    sections: [
      {
        titleKey: "settings.knowledge.mappings.form.sections.identity",
        fieldKeys: ["id", "node_type_id"],
      },
      {
        titleKey: "settings.knowledge.mappings.form.sections.source",
        fieldKeys: ["source_id", "relation", "relation_kind"],
      },
      {
        titleKey: "settings.knowledge.mappings.form.sections.keys",
        fieldKeys: ["primary_key_columns", "partition_columns"],
      },
      {
        titleKey: "settings.knowledge.mappings.form.sections.scope",
        fieldKeys: ["row_filter", "workspace_scope"],
        defaultOpen: false,
      },
      {
        titleKey: "settings.knowledge.mappings.form.sections.lifecycle",
        fieldKeys: ["valid_from", "valid_to", "precedence"],
        defaultOpen: false,
      },
      {
        titleKey: "settings.knowledge.mappings.form.sections.cache",
        fieldKeys: ["cache_hint"],
        defaultOpen: false,
      },
      {
        titleKey: "settings.knowledge.mappings.form.sections.propertyMappings",
        fieldKeys: ["property_mappings"],
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
      placeholder: "om-customer",
      readOnly: (record) =>
        record.id !== "" &&
        record.id !== objectMappingSchema.buildDefault().id,
    },
    {
      kind: "ref",
      key: "node_type_id",
      labelKey: "nodeTypeId",
      required: true,
      entityKind: "node_type",
    },
    {
      kind: "ref",
      key: "source_id",
      labelKey: "sourceId",
      required: true,
      entityKind: "source",
    },
    {
      kind: "text",
      key: "relation",
      labelKey: "relation",
      required: true,
      monospace: true,
      placeholder: "public.customers",
    },
    {
      kind: "enum",
      key: "relation_kind",
      labelKey: "relationKind",
      options: SOURCE_RELATION_KIND_OPTIONS,
    },
    columnRefListField(
      "primary_key_columns",
      "primaryKeyColumns",
      "primaryKeyColumnsDescription",
    ),
    columnRefListField(
      "partition_columns",
      "partitionColumns",
      "partitionColumnsDescription",
    ),
    {
      kind: "text",
      key: "row_filter",
      labelKey: "rowFilter",
      descriptionKey: "rowFilterDescription",
      multiline: true,
      monospace: true,
      placeholder: "deleted_at IS NULL",
    },
    {
      kind: "nested",
      key: "workspace_scope",
      labelKey: "workspaceScope",
      descriptionKey: "workspaceScopeDescription",
      schema: columnRefAsUnknown,
    },
    {
      kind: "datetime",
      key: "valid_from",
      labelKey: "validFrom",
    },
    {
      kind: "datetime",
      key: "valid_to",
      labelKey: "validTo",
    },
    {
      kind: "number",
      key: "precedence",
      labelKey: "precedence",
      descriptionKey: "precedenceDescription",
      min: 0,
      step: 1,
    },
    {
      kind: "discriminated",
      key: "cache_hint",
      labelKey: "cacheHint",
      tag: "kind",
      variants: CACHE_HINT_VARIANTS,
    },
    {
      kind: "list",
      key: "property_mappings",
      labelKey: "propertyMappings",
      addLabelKey: "propertyMappingsAdd",
      emptyTitleKey: "propertyMappingsEmptyTitle",
      emptyDescriptionKey: "propertyMappingsEmptyDescription",
      itemSchema: propertyMappingItemSchema as EntitySchema<unknown>,
      newItem: () => propertyMappingItemSchema.buildDefault(),
      itemKey: (item, idx) => {
        const p = item as PropertyMappingDef;
        return p.property_id || `pm-${idx}`;
      },
      rowPreview: (item) => {
        const p = item as PropertyMappingDef;
        return (
          <span>
            <span className="font-mono text-2xs text-foreground-strong">
              {p.property_key || p.property_id || "—"}
            </span>
            <span className="ms-2 text-foreground-muted">
              ← {p.location?.kind ?? "?"}
            </span>
          </span>
        );
      },
    } as FieldSchema<ObjectMappingDef>,
  ],
};

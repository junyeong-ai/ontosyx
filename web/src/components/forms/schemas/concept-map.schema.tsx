import type { ConceptMapDef } from "@/types/ontology";
import type { ConceptMapping } from "@/types/ontology";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";

// ConceptMap entity schema. Models a directional translation
// table from one CodeSystem to another. The mapping rows
// (source_code → target_code with equivalence) are managed via the
// nested `mappings` list, which uses the ref-autocomplete-driven
// composition pattern shared with ValueSet.

const conceptMappingItemSchema: EntitySchema<ConceptMapping> = {
  entityKind: "settings.vocabulary.conceptMaps.form.mapping",
  buildDefault: () => ({
    source_code: "",
    target_code: "",
    equivalence: "equivalent",
    comment: { default: "" },
  }),
  fields: [
    {
      kind: "text",
      key: "source_code",
      labelKey: "sourceCode",
      required: true,
      placeholder: "A",
      monospace: true,
    },
    {
      kind: "text",
      key: "target_code",
      labelKey: "targetCode",
      required: true,
      placeholder: "ACTIVE",
      monospace: true,
    },
    {
      kind: "enum",
      key: "equivalence",
      labelKey: "equivalence",
      required: true,
      options: [
        { value: "equivalent", labelKey: "equivalenceOptions.equivalent" },
        { value: "narrower_than_target", labelKey: "equivalenceOptions.narrower_than_target" },
        { value: "broader_than_target", labelKey: "equivalenceOptions.broader_than_target" },
        { value: "related", labelKey: "equivalenceOptions.related" },
        { value: "disjoint", labelKey: "equivalenceOptions.disjoint" },
      ],
    },
    {
      kind: "localized",
      key: "comment",
      labelKey: "comment",
    },
  ],
};

export const conceptMapSchema: EntitySchema<ConceptMapDef> = {
  entityKind: "settings.vocabulary.conceptMaps.form",
  buildDefault: () => ({
    id: "",
    name: "",
    display_name: { default: "" },
    description: { default: "" },
    version: "1",
    source_system_id: "",
    target_system_id: "",
    mappings: [],
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
        titleKey: "settings.vocabulary.conceptMaps.form.sections.identity",
        fieldKeys: ["id", "name", "display_name"],
      },
      {
        titleKey: "settings.vocabulary.conceptMaps.form.sections.metadata",
        fieldKeys: ["description", "version"],
      },
      {
        titleKey: "settings.vocabulary.conceptMaps.form.sections.systems",
        fieldKeys: ["source_system_id", "target_system_id"],
      },
      {
        titleKey: "settings.vocabulary.conceptMaps.form.sections.mappings",
        fieldKeys: ["mappings"],
      },
    ],
  },
  fields: [
    {
      kind: "text",
      key: "id",
      labelKey: "id",
      required: true,
      placeholder: "cm-iso3166-iso2",
      monospace: true,
      readOnly: (record) =>
        record.id !== "" && record.id !== conceptMapSchema.buildDefault().id,
    },
    {
      kind: "text",
      key: "name",
      labelKey: "name",
      required: true,
    },
    {
      kind: "localized",
      key: "display_name",
      labelKey: "displayName",
    },
    {
      kind: "localized",
      key: "description",
      labelKey: "description",
    },
    {
      kind: "text",
      key: "version",
      labelKey: "version",
      required: true,
      placeholder: "1",
      monospace: true,
    },
    {
      kind: "ref",
      key: "source_system_id",
      labelKey: "sourceSystemId",
      entityKind: "code_system",
      required: true,
    },
    {
      kind: "ref",
      key: "target_system_id",
      labelKey: "targetSystemId",
      entityKind: "code_system",
      required: true,
    },
    {
      kind: "list",
      key: "mappings",
      labelKey: "mappings",
      addLabelKey: "mappingsAdd",
      emptyTitleKey: "mappingsEmptyTitle",
      emptyDescriptionKey: "mappingsEmptyDescription",
      itemSchema: conceptMappingItemSchema as EntitySchema<unknown>,
      newItem: () => conceptMappingItemSchema.buildDefault(),
      itemKey: (item, idx) => {
        const mapping = item as ConceptMapping;
        return `${mapping.source_code || "source"}:${mapping.target_code || "target"}:${idx}`;
      },
      rowPreview: (item) => {
        const mapping = item as ConceptMapping;
        return (
          <span>
            <span className="font-mono text-2xs text-foreground-strong">
              {mapping.source_code || "?"}
            </span>
            <span className="mx-1 text-foreground-muted">→</span>
            <span className="font-mono text-2xs text-foreground-strong">
              {mapping.target_code || "?"}
            </span>
            <span className="ms-2 text-foreground-muted">
              {mapping.equivalence}
            </span>
          </span>
        );
      },
    } as FieldSchema<ConceptMapDef>,
  ],
};

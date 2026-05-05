import type { ConceptMapDef } from "@/types/ontology";
import type { EntitySchema } from "@/lib/forms/field-schema";

// ConceptMap entity schema. Models a directional translation
// table from one CodeSystem to another. The mapping rows
// (source_code → target_code with equivalence) are managed via the
// nested `mappings` list, which uses the ref-autocomplete-driven
// composition pattern shared with ValueSet.

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
  ],
};

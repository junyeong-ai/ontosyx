import type { NotationPatternDef } from "@/lib/api/edit-ops";
import type { EntitySchema } from "@/lib/forms/field-schema";

// NotationPattern entity schema. The `components` array is a
// dynamic mini-DSL describing each segment of the structured
// identifier (literal / sequence / year / ...). The structured
// renderer covers the canonical metadata; segment authoring lives
// behind the JSON tab until the bespoke component builder lands as
// a custom field.

export const notationPatternSchema: EntitySchema<NotationPatternDef> = {
  entityKind: "settings.vocabulary.notationPatterns.form",
  buildDefault: () => ({
    id: "",
    name: "",
    display_name: { default: "" },
    description: { default: "" },
    template: "",
    separator: "_",
    components: [],
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
        titleKey: "settings.vocabulary.notationPatterns.form.sections.identity",
        fieldKeys: ["id", "name", "display_name"],
      },
      {
        titleKey:
          "settings.vocabulary.notationPatterns.form.sections.template",
        fieldKeys: ["template", "separator", "description"],
      },
    ],
  },
  fields: [
    {
      kind: "text",
      key: "id",
      labelKey: "id",
      required: true,
      placeholder: "np-campaign-code",
      monospace: true,
      readOnly: (record) =>
        record.id !== "" &&
        record.id !== notationPatternSchema.buildDefault().id,
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
      kind: "text",
      key: "template",
      labelKey: "template",
      required: true,
      placeholder: "{{campaign}}_{{year}}_{{seq}}",
      monospace: true,
    },
    {
      kind: "text",
      key: "separator",
      labelKey: "separator",
      placeholder: "_",
      monospace: true,
    },
    {
      kind: "localized",
      key: "description",
      labelKey: "description",
    },
  ],
};

import type { ValueSetDef } from "@/lib/api/edit-ops";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";

// ValueSet entity schema. Composition (system_id + selector + mode)
// is a list of mini-records each combining a CodeSystem ref + a
// selector discriminant + an inclusion mode. Treated as a generic
// list of objects; a richer composition builder lands as a custom
// field once the ref autocomplete contract is wired.

type CompositionEntry = NonNullable<ValueSetDef["composition"]>[number];

const compositionItemSchema: EntitySchema<CompositionEntry> = {
  entityKind: "settings.vocabulary.valueSets.form.composition",
  buildDefault: () => ({
    system_id: "",
    selector: { kind: "all" },
    mode: "include",
  }),
  fields: [
    {
      kind: "ref",
      key: "system_id",
      labelKey: "systemId",
      entityKind: "code_system",
      required: true,
    },
    {
      kind: "enum",
      key: "mode",
      labelKey: "mode",
      required: true,
      options: [
        { value: "include", labelKey: "modeOptions.include" },
        { value: "exclude", labelKey: "modeOptions.exclude" },
      ],
    },
  ],
};

export const valueSetSchema: EntitySchema<ValueSetDef> = {
  entityKind: "settings.vocabulary.valueSets.form",
  buildDefault: () => ({
    id: "",
    name: "",
    display_name: { default: "" },
    description: { default: "" },
    version: "1",
    composition: [],
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
        titleKey: "settings.vocabulary.valueSets.form.sections.identity",
        fieldKeys: ["id", "name", "display_name"],
      },
      {
        titleKey: "settings.vocabulary.valueSets.form.sections.metadata",
        fieldKeys: ["description", "version"],
      },
      {
        titleKey: "settings.vocabulary.valueSets.form.sections.composition",
        fieldKeys: ["composition"],
      },
    ],
  },
  fields: [
    {
      kind: "text",
      key: "id",
      labelKey: "id",
      required: true,
      placeholder: "vs-order-status",
      monospace: true,
      readOnly: (record) =>
        record.id !== "" && record.id !== valueSetSchema.buildDefault().id,
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
      kind: "list",
      key: "composition",
      labelKey: "composition",
      addLabelKey: "compositionAdd",
      emptyTitleKey: "compositionEmptyTitle",
      emptyDescriptionKey: "compositionEmptyDescription",
      itemSchema: compositionItemSchema as EntitySchema<unknown>,
      newItem: () => compositionItemSchema.buildDefault(),
      itemKey: (item, idx) => {
        const c = item as CompositionEntry;
        return c.system_id || `composition-${idx}`;
      },
      rowPreview: (item) => {
        const c = item as CompositionEntry;
        return (
          <span>
            <span className="font-mono text-2xs text-foreground-strong">
              {c.system_id || "—"}
            </span>
            <span className="ms-2 text-foreground-muted">
              {c.mode}
            </span>
          </span>
        );
      },
    } as FieldSchema<ValueSetDef>,
  ],
};

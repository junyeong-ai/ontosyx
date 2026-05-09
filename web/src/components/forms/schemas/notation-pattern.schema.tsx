import type { NotationPatternDef } from "@/lib/api/edit-ops";
import type { components } from "@/types/api.generated";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";

type NotationComponent = components["schemas"]["NotationComponent"];

const codeFromSetSchema: EntitySchema<{ kind: "code_from_set"; value_set_id: string }> = {
  entityKind: "settings.vocabulary.notationPatterns.form.componentKind",
  buildDefault: () => ({ kind: "code_from_set", value_set_id: "" }),
  fields: [
    {
      kind: "ref",
      key: "value_set_id",
      labelKey: "valueSetId",
      entityKind: "value_set",
      required: true,
    },
  ],
};

const integerRangeSchema: EntitySchema<{
  kind: "integer_range";
  min: number;
  max: number;
  width: number;
}> = {
  entityKind: "settings.vocabulary.notationPatterns.form.componentKind",
  buildDefault: () => ({ kind: "integer_range", min: 0, max: 9999, width: 0 }),
  validate: (record) =>
    record.max < record.min
      ? [{ messageKey: "rangeOrder", params: { field: "max" } }]
      : [],
  fields: [
    { kind: "number", key: "min", labelKey: "min", required: true },
    { kind: "number", key: "max", labelKey: "max", required: true },
    {
      kind: "number",
      key: "width",
      labelKey: "width",
      required: true,
      min: 0,
      step: 1,
    },
  ],
};

const alphanumericSchema: EntitySchema<{
  kind: "alphanumeric";
  width: number;
  uppercase: boolean;
}> = {
  entityKind: "settings.vocabulary.notationPatterns.form.componentKind",
  buildDefault: () => ({ kind: "alphanumeric", width: 1, uppercase: true }),
  fields: [
    {
      kind: "number",
      key: "width",
      labelKey: "width",
      required: true,
      min: 1,
      step: 1,
    },
    {
      kind: "toggle",
      key: "uppercase",
      labelKey: "uppercase",
    },
  ],
};

const freeTextSchema: EntitySchema<{ kind: "free_text"; max_len?: number | null }> = {
  entityKind: "settings.vocabulary.notationPatterns.form.componentKind",
  buildDefault: () => ({ kind: "free_text", max_len: null }),
  fields: [
    {
      kind: "number",
      key: "max_len",
      labelKey: "maxLen",
      min: 1,
      step: 1,
    },
  ],
};

const componentSchema: EntitySchema<NotationComponent> = {
  entityKind: "settings.vocabulary.notationPatterns.form.component",
  buildDefault: () => ({
    name: "",
    display: { default: "" },
    kind: codeFromSetSchema.buildDefault(),
  }),
  fields: [
    {
      kind: "text",
      key: "name",
      labelKey: "name",
      required: true,
      placeholder: "campaign",
      monospace: true,
    },
    {
      kind: "localized",
      key: "display",
      labelKey: "display",
    },
    {
      kind: "discriminated",
      key: "kind",
      labelKey: "kind",
      required: true,
      tag: "kind",
      variants: {
        code_from_set: codeFromSetSchema as EntitySchema<unknown>,
        integer_range: integerRangeSchema as EntitySchema<unknown>,
        alphanumeric: alphanumericSchema as EntitySchema<unknown>,
        free_text: freeTextSchema as EntitySchema<unknown>,
      },
    },
  ],
};

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
    if (record.components.length === 0) {
      issues.push({ messageKey: "required", params: { field: "components" } });
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
      {
        titleKey:
          "settings.vocabulary.notationPatterns.form.sections.components",
        fieldKeys: ["components"],
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
    {
      kind: "list",
      key: "components",
      labelKey: "components",
      addLabelKey: "componentsAdd",
      emptyTitleKey: "componentsEmptyTitle",
      emptyDescriptionKey: "componentsEmptyDescription",
      itemSchema: componentSchema as EntitySchema<unknown>,
      newItem: () => componentSchema.buildDefault(),
      itemKey: (item, idx) => {
        const component = item as NotationComponent;
        return component.name || `component-${idx}`;
      },
      rowPreview: (item) => {
        const component = item as NotationComponent;
        return (
          <span>
            <span className="font-mono text-2xs text-foreground-strong">
              {component.name || "—"}
            </span>
            <span className="ms-2 text-foreground-muted">
              {component.kind.kind}
            </span>
          </span>
        );
      },
    } as FieldSchema<NotationPatternDef>,
  ],
};

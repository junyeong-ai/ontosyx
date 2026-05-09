import type { CodeSystemDef, CodedValue } from "@/lib/api/edit-ops";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";

// CodeSystem entity schema. The CodeSystem wire shape mirrors the
// Rust internally-tagged enum for `kind` (`{"kind": "internal"}` or
// `{"kind": "external", "source_ref": ...}`). Schema lives next to the
// other vocabulary entity schemas so adding a new entity is a single
// file change + i18n addition.

const internalKindSchema: EntitySchema<{ kind: "internal" }> = {
  entityKind: "settings.vocabulary.codeSystems.form.kindOptions",
  buildDefault: () => ({ kind: "internal" }),
  fields: [],
};

const externalKindSchema: EntitySchema<{ kind: "external"; source_ref: string }> = {
  entityKind: "settings.vocabulary.codeSystems.form.kindOptions",
  buildDefault: () => ({ kind: "external", source_ref: "" }),
  fields: [
    {
      kind: "text",
      key: "source_ref",
      labelKey: "sourceRef",
      required: true,
      placeholder: "urn:iso:std:iso:3166",
      monospace: true,
    },
  ],
};

const codedValueItemSchema: EntitySchema<CodedValue> = {
  entityKind: "settings.vocabulary.codeSystems.form.code",
  buildDefault: () => ({
    id: "",
    code: "",
    display: { default: "" },
  }),
  fields: [
    {
      kind: "text",
      key: "id",
      labelKey: "id",
      required: true,
      placeholder: "cv-active",
      monospace: true,
    },
    {
      kind: "text",
      key: "code",
      labelKey: "code",
      required: true,
      placeholder: "ACTIVE",
      monospace: true,
    },
    {
      kind: "localized",
      key: "display",
      labelKey: "display",
      required: true,
    },
    {
      kind: "localized",
      key: "definition",
      labelKey: "definition",
    },
    {
      kind: "datetime",
      key: "deprecated_at",
      labelKey: "deprecatedAt",
    },
  ],
};

export const codeSystemSchema: EntitySchema<CodeSystemDef> = {
  entityKind: "settings.vocabulary.codeSystems.form",
  buildDefault: () => ({
    id: "",
    name: "",
    display_name: { default: "" },
    description: { default: "" },
    version: "1",
    kind: internalKindSchema.buildDefault() as CodeSystemDef["kind"],
    hierarchical: false,
    codes: [],
  }),
  validate: (record) => {
    const issues = [];
    if (record.id && !/^[a-z][a-z0-9:_-]*$/i.test(record.id)) {
      issues.push({
        messageKey: "idFormat",
        params: { field: "id" },
      });
    }
    return issues;
  },
  layout: {
    sections: [
      {
        titleKey: "settings.vocabulary.codeSystems.form.sections.identity",
        fieldKeys: ["id", "name", "display_name"],
      },
      {
        titleKey: "settings.vocabulary.codeSystems.form.sections.metadata",
        fieldKeys: ["description", "version", "kind", "hierarchical"],
      },
      {
        titleKey: "settings.vocabulary.codeSystems.form.sections.codes",
        fieldKeys: ["codes"],
      },
    ],
  },
  fields: [
    {
      kind: "text",
      key: "id",
      labelKey: "id",
      required: true,
      placeholder: "cs-order-status",
      monospace: true,
      readOnly: (record) =>
        record.id !== "" && record.id !== codeSystemSchema.buildDefault().id,
    },
    {
      kind: "text",
      key: "name",
      labelKey: "name",
      required: true,
      placeholder: "OrderStatus",
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
      kind: "discriminated",
      key: "kind",
      labelKey: "kind",
      required: true,
      tag: "kind",
      variants: {
        internal: internalKindSchema as EntitySchema<unknown>,
        external: externalKindSchema as EntitySchema<unknown>,
      },
    },
    {
      kind: "toggle",
      key: "hierarchical",
      labelKey: "hierarchical",
      descriptionKey: "hierarchicalDescription",
    },
    {
      kind: "list",
      key: "codes",
      labelKey: "codes",
      addLabelKey: "codesAdd",
      emptyTitleKey: "codesEmptyTitle",
      emptyDescriptionKey: "codesEmptyDescription",
      itemSchema: codedValueItemSchema as EntitySchema<unknown>,
      newItem: () => codedValueItemSchema.buildDefault(),
      itemKey: (item, idx) => {
        const cv = item as CodedValue;
        return cv.id || `code-${idx}`;
      },
      rowPreview: (item) => {
        const cv = item as CodedValue;
        return (
          <span>
            <span className="font-mono text-2xs text-foreground-strong">
              {cv.code || "—"}
            </span>
            <span className="ms-2 text-foreground-muted">
              {cv.display?.default || cv.id || "(unnamed)"}
            </span>
          </span>
        );
      },
    } as FieldSchema<CodeSystemDef>,
  ],
};

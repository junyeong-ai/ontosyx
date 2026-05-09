import { useTranslations } from "next-intl";

import { FormInput, FormSelect } from "@/components/ui/form-input";
import { ChipInput } from "@/components/ui/chip-input";
import type { ValueSetDef } from "@/lib/api/edit-ops";
import type { EntitySchema, FieldSchema } from "@/lib/forms/field-schema";

// ValueSet entity schema. Composition (system_id + selector + mode)
// is a list of mini-records each combining a CodeSystem ref + a
// selector discriminant + an inclusion mode. Treated as a generic
// list of objects; a richer composition builder lands as a custom
// field once the ref autocomplete contract is wired.

type CompositionEntry = NonNullable<ValueSetDef["composition"]>[number];
type ValueSetSelector = CompositionEntry["selector"];

function defaultSelector(kind: ValueSetSelector["kind"]): ValueSetSelector {
  switch (kind) {
    case "explicit":
      return { kind, codes: [] };
    case "descendants_of":
      return { kind, root_id: "" };
    case "code_pattern":
      return { kind, pattern: "" };
    case "all":
      return { kind };
  }
}

function normalizeSelector(value: unknown): ValueSetSelector {
  if (value && typeof value === "object" && "kind" in value) {
    return value as ValueSetSelector;
  }
  return { kind: "all" };
}

function ValueSetSelectorControl({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const t = useTranslations("settings.vocabulary.valueSets.form");
  const selector = normalizeSelector(value);

  return (
    <div className="flex flex-col gap-2">
      <FormSelect
        value={selector.kind}
        onChange={(event) =>
          onChange(defaultSelector(event.target.value as ValueSetSelector["kind"]))
        }
        density="compact"
      >
        <option value="all">{t("selectorOptions.all")}</option>
        <option value="explicit">{t("selectorOptions.explicit")}</option>
        <option value="descendants_of">
          {t("selectorOptions.descendants_of")}
        </option>
        <option value="code_pattern">{t("selectorOptions.code_pattern")}</option>
      </FormSelect>

      {selector.kind === "explicit" && (
        <ChipInput
          values={selector.codes}
          onChange={(codes) => onChange({ ...selector, codes })}
          placeholder={t("codePlaceholder")}
          ariaLabel={t("codes")}
          monospace
        />
      )}
      {selector.kind === "descendants_of" && (
        <FormInput
          value={selector.root_id}
          onChange={(event) =>
            onChange({ ...selector, root_id: event.target.value })
          }
          placeholder={t("rootIdPlaceholder")}
          density="compact"
          className="font-mono"
        />
      )}
      {selector.kind === "code_pattern" && (
        <FormInput
          value={selector.pattern}
          onChange={(event) =>
            onChange({ ...selector, pattern: event.target.value })
          }
          placeholder="^A-"
          density="compact"
          className="font-mono"
        />
      )}
    </div>
  );
}

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
    {
      kind: "custom",
      key: "selector",
      labelKey: "selector",
      required: true,
      validate: (value) => {
        const selector = normalizeSelector(value);
        if (selector.kind === "explicit" && selector.codes.length === 0) {
          return { messageKey: "required", params: { field: "codes" } };
        }
        if (
          selector.kind === "descendants_of" &&
          selector.root_id.trim().length === 0
        ) {
          return { messageKey: "required", params: { field: "root_id" } };
        }
        if (
          selector.kind === "code_pattern" &&
          selector.pattern.trim().length === 0
        ) {
          return { messageKey: "required", params: { field: "pattern" } };
        }
        return null;
      },
      render: ({ value, onChange }) => (
        <ValueSetSelectorControl value={value} onChange={onChange} />
      ),
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

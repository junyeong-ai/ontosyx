"use client";

import { useId } from "react";
import { useTranslations } from "next-intl";

import {
  FormInput,
  FormSelect,
  FormTextarea,
  SettingsSwitch,
} from "@/components/ui/form-input";
import { FormSection } from "@/components/forms/form-section";
import type {
  EntitySchema,
  FieldError,
  FieldSchema,
} from "@/lib/forms/field-schema";

import { DateTimeInput } from "./primitives/datetime-input";
import { ListBuilder } from "./primitives/list-builder";
import { LocalizedTextInput } from "./primitives/localized-text-input";

// StructuredForm — generic schema-driven entity editor.
//
// Renders a form for any entity that supplies an
// [`EntitySchema`](../../lib/forms/field-schema). Each field's `kind`
// dispatches to the matching primitive; `discriminated` recurses into
// the matching variant schema; `list` recurses into a per-row form.
// The renderer is layout-aware: when the schema declares
// `layout.sections`, fields are grouped under collapsible sections
// in the listed order; otherwise everything renders as a flat
// column.
//
// Validation: field-level errors come in through `errors` (keyed by
// the schema's `key`). Form-level errors (cross-field) carry the
// reserved key `"_form"` and render at the top.

interface StructuredFormProps<T> {
  schema: EntitySchema<T>;
  value: T;
  onChange: (next: T) => void;
  errors?: ReadonlyMap<string, FieldError>;
  disabled?: boolean;
}

export function StructuredForm<T>({
  schema,
  value,
  onChange,
  errors,
  disabled,
}: StructuredFormProps<T>) {
  const updateField = (key: keyof T & string, next: unknown) => {
    onChange({ ...value, [key]: next });
  };

  if (schema.layout) {
    return (
      <SectionedFields
        schema={schema}
        value={value}
        onChange={updateField}
        errors={errors}
        disabled={disabled}
      />
    );
  }

  return (
    <FieldGrid
      fields={schema.fields}
      value={value}
      onChange={updateField}
      errors={errors}
      disabled={disabled}
      entityKind={schema.entityKind}
    />
  );
}

interface SectionedFieldsProps<T> {
  schema: EntitySchema<T>;
  value: T;
  onChange: (key: keyof T & string, next: unknown) => void;
  errors?: ReadonlyMap<string, FieldError>;
  disabled?: boolean;
}

function SectionedFields<T>({
  schema,
  value,
  onChange,
  errors,
  disabled,
}: SectionedFieldsProps<T>) {
  const tRoot = useTranslations();
  const layout = schema.layout;
  if (!layout) return null;
  const sectionedKeys = new Set(layout.sections.flatMap((s) => s.fieldKeys));
  const looseFields = schema.fields.filter((f) => !sectionedKeys.has(f.key));
  return (
    <div className="flex flex-col gap-3">
      {looseFields.length > 0 && (
        <FieldGrid
          fields={looseFields}
          value={value}
          onChange={onChange}
          errors={errors}
          disabled={disabled}
          entityKind={schema.entityKind}
        />
      )}
      {layout.sections.map((section) => {
        const fieldsInSection = section.fieldKeys
          .map((k) => schema.fields.find((f) => f.key === k))
          .filter((f): f is FieldSchema<T> => Boolean(f));
        return (
          <FormSection
            key={section.titleKey}
            title={tRoot(section.titleKey)}
            collapsible={section.defaultOpen === false}
            defaultOpen={section.defaultOpen ?? true}
          >
            <FieldGrid
              fields={fieldsInSection}
              value={value}
              onChange={onChange}
              errors={errors}
              disabled={disabled}
              entityKind={schema.entityKind}
            />
          </FormSection>
        );
      })}
    </div>
  );
}

interface FieldGridProps<T> {
  fields: ReadonlyArray<FieldSchema<T>>;
  value: T;
  onChange: (key: keyof T & string, next: unknown) => void;
  errors?: ReadonlyMap<string, FieldError>;
  disabled?: boolean;
  entityKind: string;
}

function FieldGrid<T>({
  fields,
  value,
  onChange,
  errors,
  disabled,
  entityKind,
}: FieldGridProps<T>) {
  return (
    <div className="grid grid-cols-1 gap-2.5">
      {fields.map((field) => {
        if (field.hidden?.(value)) return null;
        const fieldValue = (value as Record<string, unknown>)[field.key];
        const fieldError = errors?.get(field.key);
        const readOnly =
          typeof field.readOnly === "function"
            ? field.readOnly(value)
            : Boolean(field.readOnly);
        return (
          <FieldRow
            key={field.key}
            entityKind={entityKind}
            field={field}
            value={fieldValue}
            record={value}
            error={fieldError}
            readOnly={readOnly || disabled === true}
            onChange={(next) => onChange(field.key, next)}
          />
        );
      })}
    </div>
  );
}

interface FieldRowProps<T> {
  entityKind: string;
  field: FieldSchema<T>;
  value: unknown;
  record: T;
  error: FieldError | undefined;
  readOnly: boolean;
  onChange: (next: unknown) => void;
}

function FieldRow<T>({
  entityKind,
  field,
  value,
  record,
  error,
  readOnly,
  onChange,
}: FieldRowProps<T>) {
  const t = useTranslations(entityKind);
  const tForms = useTranslations("forms.errors");
  const tFormsCommon = useTranslations("forms");
  const id = useId();
  const labelText = t(field.labelKey);
  const description = field.descriptionKey ? t(field.descriptionKey) : null;
  const errorMessage = error
    ? tForms(error.messageKey, error.params ?? {})
    : null;
  const ariaInvalid = Boolean(error);

  return (
    <div className="flex flex-col gap-1">
      <label
        htmlFor={id}
        className="text-2xs font-medium text-foreground"
      >
        {labelText}
        {field.required && (
          <span
            className="ms-0.5 text-danger-foreground"
            aria-label={tFormsCommon("requiredAria")}
          >
            *
          </span>
        )}
      </label>
      {description && (
        <p className="text-2xs text-foreground-subtle">{description}</p>
      )}
      <FieldControl
        id={id}
        field={field}
        value={value}
        record={record}
        readOnly={readOnly}
        onChange={onChange}
        ariaInvalid={ariaInvalid}
        entityKind={entityKind}
      />
      {errorMessage && (
        <p className="text-2xs text-danger-foreground" role="alert">
          {errorMessage}
        </p>
      )}
    </div>
  );
}

interface FieldControlProps<T> {
  id: string;
  field: FieldSchema<T>;
  value: unknown;
  record: T;
  readOnly: boolean;
  onChange: (next: unknown) => void;
  ariaInvalid: boolean;
  entityKind: string;
}

function FieldControl<T>({
  id,
  field,
  value,
  record,
  readOnly,
  onChange,
  ariaInvalid,
  entityKind,
}: FieldControlProps<T>) {
  const t = useTranslations(entityKind);

  switch (field.kind) {
    case "text":
      if (field.multiline) {
        return (
          <FormTextarea
            id={id}
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
            placeholder={field.placeholder}
            rows={4}
            disabled={readOnly}
            aria-invalid={ariaInvalid}
            className={field.monospace ? "font-mono text-2xs" : undefined}
          />
        );
      }
      return (
        <FormInput
          id={id}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder={field.placeholder}
          density="compact"
          disabled={readOnly}
          aria-invalid={ariaInvalid}
          className={field.monospace ? "font-mono" : undefined}
        />
      );

    case "localized":
      return (
        <LocalizedTextInput
          value={value as Parameters<typeof LocalizedTextInput>[0]["value"]}
          onChange={onChange}
          disabled={readOnly}
          ariaInvalid={ariaInvalid}
        />
      );

    case "number":
      return (
        <FormInput
          id={id}
          type="number"
          value={value === undefined || value === null ? "" : String(value)}
          onChange={(e) => {
            const v = e.target.value;
            onChange(v === "" ? undefined : Number(v));
          }}
          min={field.min}
          max={field.max}
          step={field.step}
          density="compact"
          disabled={readOnly}
          aria-invalid={ariaInvalid}
        />
      );

    case "toggle":
      return (
        <SettingsSwitch
          checked={Boolean(value)}
          onChange={onChange}
          disabled={readOnly}
        />
      );

    case "enum":
      return (
        <FormSelect
          id={id}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          density="compact"
          disabled={readOnly}
          aria-invalid={ariaInvalid}
        >
          {field.options.map((option) => (
            <option key={option.value} value={option.value}>
              {t(option.labelKey)}
            </option>
          ))}
        </FormSelect>
      );

    case "datetime":
      return (
        <DateTimeInput
          value={value as string | null | undefined}
          onChange={onChange}
          disabled={readOnly}
          ariaInvalid={ariaInvalid}
          ariaLabel={t(field.labelKey)}
        />
      );

    case "list": {
      const items = (value as unknown[]) ?? [];
      const subSchema = field.itemSchema;
      return (
        <ListBuilder
          items={items}
          onChange={onChange}
          itemKey={(item, idx) => field.itemKey(item, idx)}
          rowPreview={(item) =>
            field.rowPreview ? (
              field.rowPreview(item)
            ) : (
              <span className="font-mono text-2xs text-foreground-muted">
                {JSON.stringify(item).slice(0, 80)}
              </span>
            )
          }
          newItem={field.newItem}
          renderRow={({ item, onChange: onItemChange }) => (
            <StructuredForm
              schema={subSchema}
              value={item}
              onChange={onItemChange}
              disabled={readOnly}
            />
          )}
          addLabel={t(field.addLabelKey)}
          emptyTitle={t(field.emptyTitleKey)}
          emptyDescription={t(field.emptyDescriptionKey)}
          disabled={readOnly}
        />
      );
    }

    case "ref":
      // Minimal first-pass — autocomplete-driven RefSelect lands when
      // the entity-loader contract is wired. For now the ref is a
      // plain text input with the entityKind hint surfaced as
      // placeholder so the operator knows the expected format.
      return (
        <FormInput
          id={id}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value || null)}
          placeholder={field.entityKind}
          density="compact"
          disabled={readOnly}
          aria-invalid={ariaInvalid}
          className="font-mono"
        />
      );

    case "discriminated": {
      const tagValue = (value as Record<string, unknown> | null | undefined)?.[
        field.tag
      ] as string | undefined;
      const variants = Object.keys(field.variants);
      const activeVariant = tagValue ?? variants[0];
      return (
        <div className="flex flex-col gap-2">
          <FormSelect
            value={activeVariant}
            onChange={(e) => {
              const next = field.variants[e.target.value]?.buildDefault();
              onChange(next);
            }}
            density="compact"
            disabled={readOnly}
          >
            {variants.map((v) => (
              <option key={v} value={v}>
                {t(`${field.labelKey}.${v}`)}
              </option>
            ))}
          </FormSelect>
          <StructuredForm
            schema={field.variants[activeVariant]}
            value={value}
            onChange={onChange}
            disabled={readOnly}
          />
        </div>
      );
    }

    case "custom":
      return field.render({ value, onChange, record });
  }
}

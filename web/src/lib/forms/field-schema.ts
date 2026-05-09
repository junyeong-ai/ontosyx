// Schema-driven form definition for IR entities.
//
// `FieldSchema<T>` declares one editable field on entity type `T` —
// its data shape, label key, validation, and any kind-specific
// affordance the renderer needs. `EntitySchema<T>` aggregates fields
// for one entity (CodeSystemDef, ValueSetDef, RuleDef, …) and is the
// single source of truth the [`StructuredForm`](../components/forms/structured-form)
// renderer reads.
//
// The pattern generalises the existing
// [`CONSTRAINT_REGISTRY`](../components/vocabulary/constraint-registry.ts)
// from per-constraint form specs to per-entity form specs. Adding a
// new entity = author one schema file; the generic renderer + Import/
// Export modal + validation pipeline flow for free.

import type { ReactNode } from "react";
import type { LocalizedText } from "@/types/ontology";

// ---------------------------------------------------------------------------
// Field kinds — the typed catalogue every schema field belongs to
// ---------------------------------------------------------------------------

export type FieldKind =
  | "text"
  | "localized"
  | "number"
  | "toggle"
  | "enum"
  | "datetime"
  | "list"
  | "ref"
  | "discriminated"
  | "nested"
  | "custom";

// ---------------------------------------------------------------------------
// Per-field shared base — what every schema entry carries regardless
// of kind
// ---------------------------------------------------------------------------

interface FieldBase<T, V> {
  /** Path into `T`. Entity forms currently use top-level fields. */
  key: keyof T & string;
  /** i18n key suffix the renderer joins under the entity's namespace
   *  to render the label. */
  labelKey: string;
  /** Optional helper text shown below the input. i18n key suffix. */
  descriptionKey?: string;
  /** When `true`, blank input fails validation. The renderer renders
   *  the asterisk affordance. */
  required?: boolean;
  /** When `true`, or when the predicate returns `true`, the field is
   *  rendered as read-only. Used for `id` on edit (locked) vs create
   *  (writeable). */
  readOnly?: boolean | ((record: T) => boolean);
  /** When the predicate returns `true`, the field is hidden. Used
   *  for conditional fields like `uri` only when `kind === external`. */
  hidden?: (record: T) => boolean;
  /** Field-level validator. Returns either `null` (valid) or an i18n
   *  key + interpolation params for the error message. The renderer
   *  surfaces the error inline + threads it into the form-level
   *  banner. */
  validate?: (value: V, record: T) => FieldError | null;
}

export interface FieldError {
  messageKey: string;
  params?: Record<string, string | number>;
}

// ---------------------------------------------------------------------------
// Kind-specific field variants
// ---------------------------------------------------------------------------

export interface TextField<T> extends FieldBase<T, string> {
  kind: "text";
  placeholder?: string;
  multiline?: boolean;
  /** Constrains the input to monospace + character class hint. */
  monospace?: boolean;
}

export interface LocalizedField<T> extends FieldBase<T, LocalizedText> {
  kind: "localized";
}

export interface NumberField<T> extends FieldBase<T, number> {
  kind: "number";
  min?: number;
  max?: number;
  step?: number;
}

export interface ToggleField<T> extends FieldBase<T, boolean> {
  kind: "toggle";
}

export interface EnumField<T, V extends string = string>
  extends FieldBase<T, V> {
  kind: "enum";
  options: readonly { value: V; labelKey: string }[];
}

export interface DateTimeField<T>
  extends FieldBase<T, string | null | undefined> {
  kind: "datetime";
}

export interface ListField<T, Item> extends FieldBase<T, Item[]> {
  kind: "list";
  /** Schema applied to every item. Items themselves act like records
   *  in their own right. */
  itemSchema: EntitySchema<Item>;
  /** Factory invoked when the operator clicks "add". */
  newItem: () => Item;
  /** Stable key per row used for React reconciliation + keyboard
   *  affordance focus. */
  itemKey: (item: Item, index: number) => string;
  /** Compact per-row preview rendered in the collapsed state. */
  rowPreview?: (item: Item) => ReactNode;
  minItems?: number;
  /** i18n key for the "add row" button label. */
  addLabelKey: string;
  /** i18n key for the "no rows" empty-state title. */
  emptyTitleKey: string;
  /** i18n key for the "no rows" empty-state description. */
  emptyDescriptionKey: string;
}

export interface RefField<T> extends FieldBase<T, string | null | undefined> {
  kind: "ref";
  /** Discriminator the autocomplete uses to fetch candidates. */
  entityKind: string;
}

export interface DiscriminatedField<T> extends FieldBase<T, unknown> {
  kind: "discriminated";
  /** The wire-format tag key, e.g. `"kind"` for `RuleKind`. */
  tag: string;
  /** Per-variant sub-schema. Keys are the tag's discriminant values. */
  variants: Readonly<Record<string, EntitySchema<unknown>>>;
}

export interface NestedField<T> extends FieldBase<T, unknown> {
  kind: "nested";
  /** Sub-schema applied to the nested object. Use for typed compound
   *  values without a discriminator (e.g., `ColumnRef { relation, column }`,
   *  `EndpointRef`). The nested schema's `entityKind` is the i18n
   *  namespace its own labels resolve under. */
  schema: EntitySchema<unknown>;
}

export interface CustomField<T> extends FieldBase<T, unknown> {
  kind: "custom";
  /** Bespoke renderer for fields that don't fit the canonical
   *  catalogue (graph picker, drag/drop reorder, etc.). The schema
   *  carries no display logic for `custom`; the entity's tab supplies
   *  it through the renderer. */
  render: (props: {
    value: unknown;
    onChange: (next: unknown) => void;
    record: T;
  }) => ReactNode;
}

export type FieldSchema<T> =
  | TextField<T>
  | LocalizedField<T>
  | NumberField<T>
  | ToggleField<T>
  | EnumField<T>
  | DateTimeField<T>
  | ListField<T, unknown>
  | RefField<T>
  | DiscriminatedField<T>
  | NestedField<T>
  | CustomField<T>;

// ---------------------------------------------------------------------------
// Entity schema — per-entity aggregation
// ---------------------------------------------------------------------------

export interface EntitySchema<T> {
  /** Stable identifier — used by the JSON Import dialog to validate
   *  the pasted payload claims to be the right entity, and as the
   *  i18n namespace prefix for label lookup. */
  entityKind: string;
  /** Default record produced when the operator opens the create
   *  dialog. */
  buildDefault: () => T;
  /** Cross-field validation that doesn't fit a single field's
   *  `validate` callback (e.g., end-of-window strictly after start). */
  validate?: (record: T) => FieldError[];
  /** Layout — sections + which fields belong to each. The renderer
   *  groups fields under collapsible sections by section, with
   *  un-grouped fields rendered first. Optional; un-sectioned schemas
   *  render as a flat column. */
  layout?: {
    sections: ReadonlyArray<{
      titleKey: string;
      fieldKeys: ReadonlyArray<keyof T & string>;
      defaultOpen?: boolean;
    }>;
  };
  /** Field definitions in render order (when no layout is supplied). */
  fields: ReadonlyArray<FieldSchema<T>>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** O(1) lookup of a field by key. */
export function findField<T>(
  schema: EntitySchema<T>,
  key: keyof T & string,
): FieldSchema<T> | undefined {
  return schema.fields.find((f) => f.key === key);
}

/** Run every field-level validator + the entity-level validator and
 *  return the flat error list. Empty list = record is valid. Descends
 *  into `nested` and `discriminated` sub-schemas so a typed compound
 *  field's invariants are part of the same flat error report. */
export function validateRecord<T>(
  schema: EntitySchema<T>,
  record: T,
): FieldError[] {
  const errors: FieldError[] = [];
  for (const field of schema.fields) {
    if (field.hidden?.(record)) continue;
    const value = (record as Record<string, unknown>)[field.key];
    if (field.required && isBlank(value)) {
      errors.push({
        messageKey: "required",
        params: { field: field.labelKey },
      });
      continue;
    }
    const fieldError = field.validate?.(value as never, record);
    if (fieldError) errors.push(fieldError);
    if (field.kind === "nested" && value != null) {
      errors.push(...validateRecord(field.schema, value));
    } else if (field.kind === "discriminated" && value != null) {
      const tag = (value as Record<string, unknown>)[field.tag] as
        | string
        | undefined;
      const variant = tag ? field.variants[tag] : undefined;
      if (variant) {
        errors.push(...validateRecord(variant, value));
      }
    } else if (field.kind === "list" && Array.isArray(value)) {
      for (const item of value) {
        errors.push(...validateRecord(field.itemSchema, item));
      }
    }
  }
  if (schema.validate) errors.push(...schema.validate(record));
  return errors;
}

function isBlank(value: unknown): boolean {
  if (value == null) return true;
  if (typeof value === "string") return value.trim().length === 0;
  if (Array.isArray(value)) return value.length === 0;
  return false;
}

/**
 * Pluggable form registry for SHACL [`ShaclConstraint`] variants.
 *
 * Each entry declares everything the rule editor needs to know about
 * a constraint kind: a stable label for the picker, the parameter
 * fields the operator fills in, and the bidirectional conversions
 * between form values and the canonical wire shape. The editor is
 * therefore *additive* — adding a new SHACL kind in the BE means
 * appending one entry here; no other UI surface changes.
 *
 * The fields are kept declarative (no JSX) so the same registry can
 * power form rendering, validation messages, list-row summaries,
 * and the help text in the constraint picker.
 */

import type {
  ConstraintTarget,
  PropertyType,
  ShaclConstraint,
} from "@/lib/api/edit-ops";

/** Datatype options exposed by [`ShaclConstraint::Datatype`]. Mirrors
 *  `ox_core::types::PropertyType` discriminants. */
export const DATATYPE_OPTIONS = [
  "string",
  "int",
  "float",
  "bool",
  "date",
  "date_time",
  "duration",
  "bytes",
] as const;

/** Field types the [`ConstraintFormField`] kind enum understands. */
export type ConstraintFieldKind =
  | "text"
  | "number"
  | "optional_number"
  | "select"
  | "value_set_id"
  | "notation_pattern_id"
  | "node_type_id"
  | "property_id"
  | "edge_label"
  | "property_key_list"
  | "constraint_target"
  | "constraint"
  | "constraint_list";

/** One field in a constraint's form schema. */
export interface ConstraintFormField {
  /** Stable key matched against the form values record. */
  key: string;
  /** Translation-key suffix under
   *  `settings.vocabulary.rules.constraints.fields.<label>`. */
  labelKey: string;
  kind: ConstraintFieldKind;
  /** When `kind === "select"`: the option set. */
  options?: readonly string[];
  /** Whether the field is required. Mirrors the BE's mandatory
   *  presence on the wire. */
  required?: boolean;
  /** Optional placeholder hint for text/number fields. */
  placeholder?: string;
}

/** Form spec for a single constraint kind. */
export interface ConstraintFormSpec<C extends ShaclConstraint = ShaclConstraint> {
  /** SHACL kind discriminant. Doubles as the i18n key for the
   *  picker label under
   *  `settings.vocabulary.rules.constraints.kinds.<kind>`. */
  kind: C["kind"];
  /** Form field schema, in render order. */
  fields: readonly ConstraintFormField[];
  /** Synthesise default form values when the operator selects this
   *  kind in the picker. */
  defaults: () => Record<string, unknown>;
  /** Hydrate form values from an existing constraint of this kind. */
  fromConstraint: (c: C) => Record<string, unknown>;
  /** Project form values back into a wire-format constraint. */
  toConstraint: (values: Record<string, unknown>) => C;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const inheritTarget: ConstraintTarget = { kind: "inherit" };

function readTarget(values: Record<string, unknown>, key = "target"): ConstraintTarget {
  const t = values[key];
  if (t && typeof t === "object" && "kind" in t) return t as ConstraintTarget;
  return inheritTarget;
}

function readNumber(values: Record<string, unknown>, key: string, fallback = 0): number {
  const v = values[key];
  if (typeof v === "number") return v;
  if (typeof v === "string") {
    const n = Number(v);
    return Number.isFinite(n) ? n : fallback;
  }
  return fallback;
}

function readString(values: Record<string, unknown>, key: string, fallback = ""): string {
  const v = values[key];
  return typeof v === "string" ? v : fallback;
}

function isScalarPropertyTypeTag(
  value: string,
): value is (typeof DATATYPE_OPTIONS)[number] {
  return DATATYPE_OPTIONS.some((option) => option === value);
}

function propertyTypeTag(type: PropertyType): (typeof DATATYPE_OPTIONS)[number] {
  return isScalarPropertyTypeTag(type.type) ? type.type : "string";
}

function readPropertyType(
  values: Record<string, unknown>,
  key: string,
  fallback: (typeof DATATYPE_OPTIONS)[number] = "string",
): PropertyType {
  const value = readString(values, key, fallback);
  return isScalarPropertyTypeTag(value)
    ? { type: value }
    : { type: fallback };
}

function readStringList(values: Record<string, unknown>, key: string): string[] {
  const v = values[key];
  if (Array.isArray(v)) return v.filter((x): x is string => typeof x === "string");
  return [];
}

export function defaultConstraint(): ShaclConstraint {
  return { kind: "min_count", target: inheritTarget, min: 1 };
}

function readConstraint(values: Record<string, unknown>, key: string): ShaclConstraint {
  const value = values[key];
  return value && typeof value === "object" && "kind" in value
    ? (value as ShaclConstraint)
    : defaultConstraint();
}

function readConstraintList(values: Record<string, unknown>, key: string): ShaclConstraint[] {
  const value = values[key];
  if (!Array.isArray(value)) return [];
  return value.filter(
    (item): item is ShaclConstraint =>
      item !== null && typeof item === "object" && "kind" in item,
  );
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

const TARGET_FIELD: ConstraintFormField = {
  key: "target",
  labelKey: "target",
  kind: "constraint_target",
  required: false,
};

/** Canonical registry — single source of truth for the rule editor. */
export const CONSTRAINT_REGISTRY: ReadonlyArray<ConstraintFormSpec> = [
  {
    kind: "min_count",
    fields: [
      TARGET_FIELD,
      { key: "min", labelKey: "min", kind: "number", required: true, placeholder: "1" },
    ],
    defaults: () => ({ target: inheritTarget, min: 1 }),
    fromConstraint: (c) =>
      c.kind === "min_count" ? { target: c.target, min: c.min } : {},
    toConstraint: (v) => ({
      kind: "min_count",
      target: readTarget(v),
      min: readNumber(v, "min", 1),
    }),
  },
  {
    kind: "max_count",
    fields: [
      TARGET_FIELD,
      { key: "max", labelKey: "max", kind: "number", required: true, placeholder: "1" },
    ],
    defaults: () => ({ target: inheritTarget, max: 1 }),
    fromConstraint: (c) =>
      c.kind === "max_count" ? { target: c.target, max: c.max } : {},
    toConstraint: (v) => ({
      kind: "max_count",
      target: readTarget(v),
      max: readNumber(v, "max", 1),
    }),
  },
  {
    kind: "datatype",
    fields: [
      TARGET_FIELD,
      {
        key: "expected",
        labelKey: "datatype",
        kind: "select",
        options: DATATYPE_OPTIONS,
        required: true,
      },
    ],
    defaults: () => ({ target: inheritTarget, expected: "string" }),
    fromConstraint: (c) =>
      c.kind === "datatype"
        ? { target: c.target, expected: propertyTypeTag(c.expected) }
        : {},
    toConstraint: (v) => ({
      kind: "datatype",
      target: readTarget(v),
      expected: readPropertyType(v, "expected"),
    }),
  },
  {
    kind: "matches_pattern",
    fields: [
      TARGET_FIELD,
      {
        key: "notation_pattern_id",
        labelKey: "notationPattern",
        kind: "notation_pattern_id",
        required: true,
      },
    ],
    defaults: () => ({ target: inheritTarget, notation_pattern_id: "" }),
    fromConstraint: (c) =>
      c.kind === "matches_pattern"
        ? { target: c.target, notation_pattern_id: c.notation_pattern_id }
        : {},
    toConstraint: (v) => ({
      kind: "matches_pattern",
      target: readTarget(v),
      notation_pattern_id: readString(v, "notation_pattern_id"),
    }),
  },
  {
    kind: "in_value_set",
    fields: [
      TARGET_FIELD,
      {
        key: "value_set_id",
        labelKey: "valueSet",
        kind: "value_set_id",
        required: true,
      },
    ],
    defaults: () => ({ target: inheritTarget, value_set_id: "" }),
    fromConstraint: (c) =>
      c.kind === "in_value_set"
        ? { target: c.target, value_set_id: c.value_set_id }
        : {},
    toConstraint: (v) => ({
      kind: "in_value_set",
      target: readTarget(v),
      value_set_id: readString(v, "value_set_id"),
    }),
  },
  {
    kind: "has_value",
    fields: [
      TARGET_FIELD,
      { key: "value", labelKey: "value", kind: "text", required: true },
    ],
    defaults: () => ({ target: inheritTarget, value: "" }),
    fromConstraint: (c) =>
      c.kind === "has_value" ? { target: c.target, value: c.value } : {},
    toConstraint: (v) => ({
      kind: "has_value",
      target: readTarget(v),
      value: readString(v, "value"),
    }),
  },
  {
    kind: "min_inclusive",
    fields: [
      TARGET_FIELD,
      { key: "min", labelKey: "min", kind: "number", required: true },
    ],
    defaults: () => ({ target: inheritTarget, min: 0 }),
    fromConstraint: (c) =>
      c.kind === "min_inclusive" ? { target: c.target, min: c.min } : {},
    toConstraint: (v) => ({
      kind: "min_inclusive",
      target: readTarget(v),
      min: readNumber(v, "min"),
    }),
  },
  {
    kind: "max_inclusive",
    fields: [
      TARGET_FIELD,
      { key: "max", labelKey: "max", kind: "number", required: true },
    ],
    defaults: () => ({ target: inheritTarget, max: 0 }),
    fromConstraint: (c) =>
      c.kind === "max_inclusive" ? { target: c.target, max: c.max } : {},
    toConstraint: (v) => ({
      kind: "max_inclusive",
      target: readTarget(v),
      max: readNumber(v, "max"),
    }),
  },
  {
    kind: "min_length",
    fields: [
      TARGET_FIELD,
      { key: "min", labelKey: "minLength", kind: "number", required: true },
    ],
    defaults: () => ({ target: inheritTarget, min: 1 }),
    fromConstraint: (c) =>
      c.kind === "min_length" ? { target: c.target, min: c.min } : {},
    toConstraint: (v) => ({
      kind: "min_length",
      target: readTarget(v),
      min: readNumber(v, "min", 1),
    }),
  },
  {
    kind: "max_length",
    fields: [
      TARGET_FIELD,
      { key: "max", labelKey: "maxLength", kind: "number", required: true },
    ],
    defaults: () => ({ target: inheritTarget, max: 255 }),
    fromConstraint: (c) =>
      c.kind === "max_length" ? { target: c.target, max: c.max } : {},
    toConstraint: (v) => ({
      kind: "max_length",
      target: readTarget(v),
      max: readNumber(v, "max", 255),
    }),
  },
  {
    kind: "unique_lang",
    fields: [TARGET_FIELD],
    defaults: () => ({ target: inheritTarget }),
    fromConstraint: (c) =>
      c.kind === "unique_lang" ? { target: c.target } : {},
    toConstraint: (v) => ({ kind: "unique_lang", target: readTarget(v) }),
  },
  {
    kind: "closed",
    fields: [
      TARGET_FIELD,
      {
        key: "allowed_properties",
        labelKey: "allowedProperties",
        kind: "property_key_list",
      },
    ],
    defaults: () => ({ target: inheritTarget, allowed_properties: [] }),
    fromConstraint: (c) =>
      c.kind === "closed"
        ? { target: c.target, allowed_properties: c.allowed_properties }
        : {},
    toConstraint: (v) => ({
      kind: "closed",
      target: readTarget(v),
      allowed_properties: readStringList(v, "allowed_properties"),
    }),
  },
  {
    kind: "disjoint",
    fields: [
      { key: "a", labelKey: "disjointA", kind: "constraint_target" },
      { key: "b", labelKey: "disjointB", kind: "constraint_target" },
    ],
    defaults: () => ({ a: inheritTarget, b: inheritTarget }),
    fromConstraint: (c) =>
      c.kind === "disjoint" ? { a: c.a, b: c.b } : {},
    toConstraint: (v) => ({
      kind: "disjoint",
      a: readTarget(v, "a"),
      b: readTarget(v, "b"),
    }),
  },
  {
    kind: "unique_key",
    fields: [
      {
        key: "target_node_type_id",
        labelKey: "nodeType",
        kind: "node_type_id",
        required: true,
      },
      {
        key: "property_keys",
        labelKey: "propertyKeys",
        kind: "property_key_list",
        required: true,
      },
    ],
    defaults: () => ({ target_node_type_id: "", property_keys: [] }),
    fromConstraint: (c) =>
      c.kind === "unique_key"
        ? {
            target_node_type_id: c.target_node_type_id,
            property_keys: c.property_keys,
          }
        : {},
    toConstraint: (v) => ({
      kind: "unique_key",
      target_node_type_id: readString(v, "target_node_type_id"),
      property_keys: readStringList(v, "property_keys"),
    }),
  },
  {
    kind: "less_than",
    fields: [
      TARGET_FIELD,
      {
        key: "other_property",
        labelKey: "otherProperty",
        kind: "property_id",
        required: true,
      },
    ],
    defaults: () => ({ target: inheritTarget, other_property: "" }),
    fromConstraint: (c) =>
      c.kind === "less_than"
        ? { target: c.target, other_property: c.other_property }
        : {},
    toConstraint: (v) => ({
      kind: "less_than",
      target: readTarget(v),
      other_property: readString(v, "other_property"),
    }),
  },
  {
    kind: "equals",
    fields: [
      TARGET_FIELD,
      {
        key: "other_property",
        labelKey: "otherProperty",
        kind: "property_id",
        required: true,
      },
    ],
    defaults: () => ({ target: inheritTarget, other_property: "" }),
    fromConstraint: (c) =>
      c.kind === "equals"
        ? { target: c.target, other_property: c.other_property }
        : {},
    toConstraint: (v) => ({
      kind: "equals",
      target: readTarget(v),
      other_property: readString(v, "other_property"),
    }),
  },
  {
    kind: "or",
    fields: [
      {
        key: "branches",
        labelKey: "branches",
        kind: "constraint_list",
      },
    ],
    defaults: () => ({ branches: [] }),
    fromConstraint: (c) => (c.kind === "or" ? { branches: c.branches } : {}),
    toConstraint: (v) => ({
      kind: "or",
      branches: readConstraintList(v, "branches"),
    }),
  },
  {
    kind: "and",
    fields: [
      {
        key: "branches",
        labelKey: "branches",
        kind: "constraint_list",
      },
    ],
    defaults: () => ({ branches: [] }),
    fromConstraint: (c) => (c.kind === "and" ? { branches: c.branches } : {}),
    toConstraint: (v) => ({
      kind: "and",
      branches: readConstraintList(v, "branches"),
    }),
  },
  {
    kind: "not",
    fields: [
      {
        key: "inner",
        labelKey: "inner",
        kind: "constraint",
        required: true,
      },
    ],
    defaults: () => ({ inner: defaultConstraint() }),
    fromConstraint: (c) => (c.kind === "not" ? { inner: c.inner } : {}),
    toConstraint: (v) => ({
      kind: "not",
      inner: readConstraint(v, "inner"),
    }),
  },
  {
    kind: "xone",
    fields: [
      {
        key: "branches",
        labelKey: "branches",
        kind: "constraint_list",
      },
    ],
    defaults: () => ({ branches: [] }),
    fromConstraint: (c) => (c.kind === "xone" ? { branches: c.branches } : {}),
    toConstraint: (v) => ({
      kind: "xone",
      branches: readConstraintList(v, "branches"),
    }),
  },
  {
    kind: "qualified_value_shape",
    fields: [
      {
        key: "shape",
        labelKey: "shape",
        kind: "constraint",
        required: true,
      },
      {
        key: "qualified_min_count",
        labelKey: "qualifiedMinCount",
        kind: "optional_number",
      },
      {
        key: "qualified_max_count",
        labelKey: "qualifiedMaxCount",
        kind: "optional_number",
      },
    ],
    defaults: () => ({ shape: defaultConstraint() }),
    fromConstraint: (c) =>
      c.kind === "qualified_value_shape"
        ? {
            shape: c.shape,
            qualified_min_count: c.qualified_min_count ?? "",
            qualified_max_count: c.qualified_max_count ?? "",
          }
        : {},
    toConstraint: (v) => {
      const min = readNumber(v, "qualified_min_count", Number.NaN);
      const max = readNumber(v, "qualified_max_count", Number.NaN);
      return {
        kind: "qualified_value_shape",
        shape: readConstraint(v, "shape"),
        ...(Number.isFinite(min) ? { qualified_min_count: min } : {}),
        ...(Number.isFinite(max) ? { qualified_max_count: max } : {}),
      };
    },
  },
];

/** O(1) lookup by `kind`. */
export function constraintSpec(
  kind: ShaclConstraint["kind"],
): ConstraintFormSpec | undefined {
  return CONSTRAINT_REGISTRY.find((spec) => spec.kind === kind);
}

/** Stable list of every supported kind. Used by the picker. */
export const CONSTRAINT_KINDS: readonly ShaclConstraint["kind"][] =
  CONSTRAINT_REGISTRY.map((spec) => spec.kind);

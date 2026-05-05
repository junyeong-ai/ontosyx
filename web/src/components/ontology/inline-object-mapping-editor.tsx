"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { Plus, X } from "lucide-react";
import type {
  ObjectMappingDef,
  PropertyMappingDef,
  PropertyLocation,
  ColumnRef,
  SourceRelationKind,
} from "@/lib/api/edit-ops";
import type { PropertyDef } from "@/types/ontology";
import { FormInput, FormSelect } from "@/components/ui/form-input";

export interface InlineObjectMappingEditorProps {
  /** Current state of the mapping. Pass a skeleton (empty `relation`,
   *  empty `property_mappings`) to render the create flow. The
   *  editor is fully controlled — every keystroke fires `onChange`
   *  with the next snapshot. */
  value: ObjectMappingDef;
  /** Properties exposed by the parent NodeType. Drives the
   *  property-mapping table — one row per property. */
  properties: readonly PropertyDef[];
  /** Optional column catalogue from the source profile. When
   *  supplied, column inputs render as datalist-backed
   *  autocomplete; otherwise they're free-form text fields. */
  availableColumns?: readonly string[];
  onChange: (next: ObjectMappingDef) => void;
  readOnly?: boolean;
}

const RELATION_KINDS: readonly SourceRelationKind[] = [
  "table",
  "view",
  "collection",
  "file",
];

/**
 * Form-based editor for one [`ObjectMappingDef`].
 *
 * The Domain Context page's Mappings section embeds this for
 * single-mapping editing — the common case where one NodeType
 * binds to one physical relation. Multi-mapping flows stay on
 * `/mappings` where the JSON dual-mode editor handles
 * the long tail.
 *
 * Stays purely controlled — caller owns the persistence boundary
 * and the id minting on first save.
 */
export function InlineObjectMappingEditor({
  value,
  properties,
  availableColumns,
  onChange,
  readOnly = false,
}: InlineObjectMappingEditorProps) {
  const t = useTranslations("ontology.inlineObjectMappingEditor");
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const propertyMappings = useMemo(
    () => indexByPropertyId(value.property_mappings ?? []),
    [value.property_mappings],
  );

  const update = (patch: Partial<ObjectMappingDef>) => {
    onChange({ ...value, ...patch });
  };

  const updatePropertyMapping = (
    property: PropertyDef,
    next: PropertyMappingDef | null,
  ) => {
    const existing = value.property_mappings ?? [];
    const filtered = existing.filter((m) => m.property_id !== property.id);
    const nextList = next ? [...filtered, next] : filtered;
    update({ property_mappings: nextList });
  };

  return (
    <div className="space-y-3">
      <FormGrid>
        <Field label={t("relationLabel")} required>
          <FormInput
            type="text"
            density="compact"
            value={value.relation ?? ""}
            onChange={(e) => update({ relation: e.target.value })}
            disabled={readOnly}
            placeholder={t("relationPlaceholder")}
          />
        </Field>
        <Field label={t("relationKindLabel")}>
          <FormSelect
            density="compact"
            value={value.relation_kind ?? "table"}
            onChange={(e) =>
              update({
                relation_kind: e.target.value as SourceRelationKind,
              })
            }
            disabled={readOnly}
          >
            {RELATION_KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {t(`relationKind.${kind}`)}
              </option>
            ))}
          </FormSelect>
        </Field>
      </FormGrid>

      <Field label={t("primaryKeyLabel")}>
        <ColumnChipInput
          value={(value.primary_key_columns ?? []).map((c) => c.column)}
          availableColumns={availableColumns}
          readOnly={readOnly}
          onChange={(columns) =>
            update({
              primary_key_columns: columns.map<ColumnRef>((column) => ({
                column,
                relation: value.relation ?? "",
              })),
            })
          }
          addLabel={t("primaryKeyAdd")}
          removeAriaTemplate={(c) => t("primaryKeyRemoveAria", { column: c })}
        />
      </Field>

      <PropertyMappingTable
        properties={properties}
        propertyMappings={propertyMappings}
        availableColumns={availableColumns}
        readOnly={readOnly}
        relation={value.relation ?? ""}
        onChange={updatePropertyMapping}
      />

      <details
        className="rounded border border-divider-soft"
        open={advancedOpen}
        onToggle={(e) => setAdvancedOpen((e.target as HTMLDetailsElement).open)}
      >
        <summary className="cursor-pointer px-2 py-1 text-2xs font-medium uppercase tracking-wider text-foreground-muted">
          {t("advancedToggle")}
        </summary>
        <div className="space-y-2 px-2 py-2">
          <Field label={t("rowFilterLabel")}>
            <FormInput
              type="text"
              density="compact"
              value={value.row_filter ?? ""}
              onChange={(e) =>
                update({ row_filter: e.target.value || null })
              }
              disabled={readOnly}
              placeholder={t("rowFilterPlaceholder")}
            />
          </Field>
          <FormGrid>
            <Field label={t("precedenceLabel")}>
              <FormInput
                type="number"
                density="compact"
                value={value.precedence ?? 0}
                onChange={(e) =>
                  update({
                    precedence: Number.isFinite(e.target.valueAsNumber)
                      ? e.target.valueAsNumber
                      : 0,
                  })
                }
                disabled={readOnly}
              />
            </Field>
          </FormGrid>
        </div>
      </details>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Property mapping table
// ---------------------------------------------------------------------------

function PropertyMappingTable({
  properties,
  propertyMappings,
  availableColumns,
  readOnly,
  relation,
  onChange,
}: {
  properties: readonly PropertyDef[];
  propertyMappings: Map<string, PropertyMappingDef>;
  availableColumns: readonly string[] | undefined;
  readOnly: boolean;
  relation: string;
  onChange: (property: PropertyDef, next: PropertyMappingDef | null) => void;
}) {
  const t = useTranslations("ontology.inlineObjectMappingEditor");

  if (properties.length === 0) {
    return (
      <p className="text-2xs italic text-foreground-muted">
        {t("noProperties")}
      </p>
    );
  }

  return (
    <div>
      <h3 className="mb-1 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("propertiesHeader")}
      </h3>
      <ul className="divide-y divide-divider-soft rounded border border-divider-soft">
        {properties.map((property) => (
          <PropertyMappingRow
            key={property.id}
            property={property}
            mapping={propertyMappings.get(property.id) ?? null}
            availableColumns={availableColumns}
            readOnly={readOnly}
            relation={relation}
            onChange={(next) => onChange(property, next)}
          />
        ))}
      </ul>
    </div>
  );
}

function PropertyMappingRow({
  property,
  mapping,
  availableColumns,
  readOnly,
  relation,
  onChange,
}: {
  property: PropertyDef;
  mapping: PropertyMappingDef | null;
  availableColumns: readonly string[] | undefined;
  readOnly: boolean;
  relation: string;
  onChange: (next: PropertyMappingDef | null) => void;
}) {
  const t = useTranslations("ontology.inlineObjectMappingEditor");
  const column = mapping ? extractColumn(mapping.location) : "";
  const isJsonPath = mapping?.location.kind === "json_path";

  const handleColumnChange = (nextColumn: string) => {
    if (nextColumn === "") {
      onChange(null);
      return;
    }
    const location: PropertyLocation = isJsonPath
      ? {
          kind: "json_path",
          root_column:
            mapping?.location.kind === "json_path"
              ? mapping.location.root_column
              : nextColumn,
          path: nextColumn,
        }
      : { kind: "column", column: nextColumn, relation };
    onChange({
      property_id: property.id,
      property_key: property.name,
      location,
      transform: mapping?.transform ?? { kind: "identity" },
      concept_map_id: mapping?.concept_map_id ?? undefined,
    });
  };

  const datalistId = `oxd-cols-${property.id}`;

  return (
    <li className="flex items-center gap-3 px-2 py-1.5">
      <span className="flex w-32 shrink-0 flex-col">
        <span className="truncate text-2xs font-medium text-foreground-strong">
          {property.name}
        </span>
        <span className="truncate text-2xs font-mono text-foreground-muted">
          {property.id}
        </span>
      </span>
      <FormInput
        type="text"
        density="compact"
        value={column}
        onChange={(e) => handleColumnChange(e.target.value)}
        disabled={readOnly}
        placeholder={t("columnPlaceholder")}
        list={availableColumns ? datalistId : undefined}
        className="flex-1"
      />
      {availableColumns && (
        <datalist id={datalistId}>
          {availableColumns.map((col) => (
            <option key={col} value={col} />
          ))}
        </datalist>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Column chip input — used for primary_key_columns
// ---------------------------------------------------------------------------

function ColumnChipInput({
  value,
  availableColumns,
  readOnly,
  onChange,
  addLabel,
  removeAriaTemplate,
}: {
  value: readonly string[];
  availableColumns: readonly string[] | undefined;
  readOnly: boolean;
  onChange: (next: string[]) => void;
  addLabel: string;
  removeAriaTemplate: (column: string) => string;
}) {
  const [draft, setDraft] = useState("");
  const datalistId = `oxd-pk-cols`;

  const commit = (column: string) => {
    const trimmed = column.trim();
    if (!trimmed) return;
    if (value.includes(trimmed)) {
      setDraft("");
      return;
    }
    onChange([...value, trimmed]);
    setDraft("");
  };

  const remove = (column: string) => {
    onChange(value.filter((c) => c !== column));
  };

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {value.map((column) => (
        <span
          key={column}
          className="inline-flex items-center gap-1 rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-mono text-foreground-muted"
        >
          {column}
          {!readOnly && (
            <button
              type="button"
              onClick={() => remove(column)}
              aria-label={removeAriaTemplate(column)}
              className="rounded p-0.5 hover:bg-surface-inset"
            >
              <X className="h-2 w-2" />
            </button>
          )}
        </span>
      ))}
      {!readOnly && (
        <span className="inline-flex items-center gap-1">
          <FormInput
            type="text"
            density="compact"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit(draft);
              }
            }}
            placeholder={addLabel}
            list={availableColumns ? datalistId : undefined}
            className="w-24 border-dashed bg-transparent"
          />
          {availableColumns && (
            <datalist id={datalistId}>
              {availableColumns.map((col) => (
                <option key={col} value={col} />
              ))}
            </datalist>
          )}
          <button
            type="button"
            onClick={() => commit(draft)}
            disabled={!draft.trim()}
            className="rounded p-0.5 text-foreground-muted hover:bg-surface-inset hover:text-concept-foreground disabled:opacity-50"
          >
            <Plus className="h-2.5 w-2.5" />
          </button>
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

function Field({
  label,
  required,
  children,
}: {
  label: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {label}
        {required && <span className="ms-0.5 text-danger-foreground">*</span>}
      </span>
      {children}
    </label>
  );
}

function FormGrid({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-2 gap-3">{children}</div>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function indexByPropertyId(
  mappings: readonly PropertyMappingDef[],
): Map<string, PropertyMappingDef> {
  const map = new Map<string, PropertyMappingDef>();
  for (const m of mappings) map.set(m.property_id, m);
  return map;
}

function extractColumn(location: PropertyLocation): string {
  switch (location.kind) {
    case "column":
      return location.column;
    case "json_path":
      return location.path;
  }
}

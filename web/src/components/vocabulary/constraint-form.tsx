"use client";

import { useTranslations } from "next-intl";

import { FormSelect, SettingsInput } from "@/components/ui/form-input";
import type { ConstraintTarget, ShaclConstraint } from "@/lib/api/edit-ops";
import { Button } from "@/components/ui/button";

import { ConstraintTargetField } from "./constraint-target-field";
import {
  CONSTRAINT_KINDS,
  CONSTRAINT_REGISTRY,
  constraintSpec,
  type ConstraintFormField,
} from "./constraint-registry";

interface ConstraintFormProps {
  /** The constraint being edited; the parent owns the value and
   *  swaps it on every change. */
  value: ShaclConstraint;
  onChange: (next: ShaclConstraint) => void;
  onRemove: () => void;
}

/**
 * Renders one [`ShaclConstraint`] as an editable form derived from
 * the [`CONSTRAINT_REGISTRY`]. Switching the kind picker rebuilds
 * the value via the new spec's [`ConstraintFormSpec.defaults`] —
 * the parent never has to know which fields exist on which kind.
 */
export function ConstraintForm({
  value,
  onChange,
  onRemove,
}: ConstraintFormProps) {
  const t = useTranslations("settings.vocabulary.rules.constraints");
  const spec = constraintSpec(value.kind);

  const handleKindChange = (nextKind: ShaclConstraint["kind"]) => {
    const nextSpec = constraintSpec(nextKind);
    if (!nextSpec) return;
    onChange(
      nextSpec.toConstraint(nextSpec.defaults()) as ShaclConstraint,
    );
  };

  if (!spec) {
    // Unrecognised kind — keep the rule editable but warn the
    // operator that this constraint won't render correctly. The
    // wire shape passes through unchanged via `onChange`.
    return (
      <div className="rounded border border-warning-border bg-warning-surface p-2 text-xs text-warning-foreground/30">
        {t("unknownKind", { kind: value.kind })}
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="ms-2"
          onClick={onRemove}
        >
          {t("remove")}
        </Button>
      </div>
    );
  }

  // Hydrate form values from the current constraint, project back on
  // every field edit. This keeps the parent's `value` always in
  // wire-shape — no intermediate "form draft" state to keep in sync.
  const formValues = spec.fromConstraint(value);
  const updateField = (key: string, fieldValue: unknown) => {
    const nextValues = { ...formValues, [key]: fieldValue };
    onChange(spec.toConstraint(nextValues) as ShaclConstraint);
  };

  return (
    <div className="flex flex-col gap-2 rounded border border-divider bg-surface-raised p-3">
      <div className="flex items-center justify-between gap-2">
        <FormSelect
          value={value.kind}
          onChange={(e) =>
            handleKindChange(e.target.value as ShaclConstraint["kind"])
          }
          density="compact"
          className="w-auto"
        >
          {CONSTRAINT_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {t(`kinds.${kind}`)}
            </option>
          ))}
        </FormSelect>
        <Button type="button" variant="ghost" size="xs" onClick={onRemove}>
          {t("remove")}
        </Button>
      </div>

      <div className="flex flex-col gap-2">
        {spec.fields.map((field) => (
          <FieldRenderer
            key={field.key}
            field={field}
            value={formValues[field.key]}
            onChange={(v) => updateField(field.key, v)}
          />
        ))}
      </div>
    </div>
  );
}

interface FieldRendererProps {
  field: ConstraintFormField;
  value: unknown;
  onChange: (next: unknown) => void;
}

function FieldRenderer({ field, value, onChange }: FieldRendererProps) {
  const t = useTranslations(
    "settings.vocabulary.rules.constraints.fields",
  );
  const label = t(field.labelKey);

  switch (field.kind) {
    case "text":
    case "value_set_id":
    case "notation_pattern_id":
    case "node_type_id":
    case "edge_label":
      return (
        <SettingsInput
          label={label}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder={field.placeholder}
          required={field.required}
        />
      );
    case "number":
      return (
        <SettingsInput
          label={label}
          type="number"
          value={value === undefined || value === null ? "" : String(value)}
          onChange={(e) => {
            const n = Number(e.target.value);
            onChange(Number.isFinite(n) ? n : 0);
          }}
          placeholder={field.placeholder}
          required={field.required}
        />
      );
    case "select":
      return (
        <label className="flex flex-col gap-1 text-xs text-foreground-muted">
          <span className="font-medium">{label}</span>
          <FormSelect
            value={typeof value === "string" ? value : ""}
            onChange={(e) => onChange(e.target.value)}
            density="compact"
          >
            {(field.options ?? []).map((opt) => (
              <option key={opt} value={opt}>
                {opt}
              </option>
            ))}
          </FormSelect>
        </label>
      );
    case "property_key_list":
      return (
        <SettingsInput
          label={label}
          value={Array.isArray(value) ? (value as string[]).join(", ") : ""}
          onChange={(e) =>
            onChange(
              e.target.value
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean),
            )
          }
          // i18n-audit-ignore — column-name example, language-neutral identifier
          placeholder="email, tenant_id"
          required={field.required}
        />
      );
    case "constraint_target":
    case "constraint_target_pair":
      return (
        <ConstraintTargetField
          label={label}
          value={(value as ConstraintTarget) ?? { kind: "inherit" }}
          onChange={onChange}
        />
      );
  }
}

/** Minimal "Add constraint" picker — appends a default constraint
 *  using the registry's `defaults()` factory. Surfaced separately
 *  so the parent rule editor can place it wherever fits. */
export function AddConstraintMenu({
  onAdd,
}: {
  onAdd: (c: ShaclConstraint) => void;
}) {
  const t = useTranslations("settings.vocabulary.rules.constraints");
  return (
    <div className="flex items-center gap-2">
      <FormSelect
        defaultValue=""
        onChange={(e) => {
          const kind = e.target.value as ShaclConstraint["kind"];
          if (!kind) return;
          const spec = CONSTRAINT_REGISTRY.find((s) => s.kind === kind);
          if (!spec) return;
          onAdd(spec.toConstraint(spec.defaults()) as ShaclConstraint);
          e.target.value = "";
        }}
        density="compact"
        className="w-auto"
      >
        <option value="">{t("addPlaceholder")}</option>
        {CONSTRAINT_KINDS.map((kind) => (
          <option key={kind} value={kind}>
            {t(`kinds.${kind}`)}
          </option>
        ))}
      </FormSelect>
    </div>
  );
}

"use client";

import { useState, type FormEvent } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import {
  SettingsInput,
  SettingsTextarea,
} from "@/components/ui/form-input";
import type {
  EnforcementKind,
  RuleActivationKind,
  RuleDef,
  RuleKind,
  ShaclConstraint,
  Severity,
} from "@/lib/api/edit-ops";

import { AddConstraintMenu, ConstraintForm } from "./constraint-form";

const SEVERITY_OPTIONS: Severity[] = ["violation", "warning", "info"];
const ENFORCEMENT_OPTIONS: EnforcementKind[] = ["write", "read", "batch"];
const ACTIVATION_OPTIONS: RuleActivationKind["kind"][] = [
  "always",
  "on_action",
  "on_schedule",
];
const RULE_KIND_OPTIONS: RuleKind["kind"][] = [
  "node_shape",
  "property_shape",
  "edge_shape",
  "cross_entity_shape",
  "state_machine",
];

interface RuleFormProps {
  /** Initial rule when editing; `undefined` produces a blank
   *  create form. */
  initial?: RuleDef;
  onSubmit: (def: RuleDef) => void;
  onCancel: () => void;
  pending?: boolean;
}

/**
 * Full SHACL [`RuleDef`] editor — pulls together the
 * Severity/Enforcement/Activation matrix, the [`RuleKind`] picker,
 * and the constraint-kind-pluggable form list. The form preserves
 * the canonical wire shape end-to-end: every change updates a
 * `RuleDef` in `useState` and the parent receives that exact
 * shape on submit.
 *
 * Derived rules (`origin.kind === "derived_from_binding"`) are
 * read-only — the form locks every control and surfaces a notice
 * directing the operator to the source binding instead.
 */
export function RuleForm({
  initial,
  onSubmit,
  onCancel,
  pending = false,
}: RuleFormProps) {
  const t = useTranslations("settings.vocabulary.rules.form");
  const isDerived = initial?.origin?.kind === "derived_from_binding";

  const [id, setId] = useState(initial?.id ?? "");
  const [nameDefault, setNameDefault] = useState(initial?.name?.default ?? "");
  const [descDefault, setDescDefault] = useState(
    initial?.description?.default ?? "",
  );
  const [rationaleDefault, setRationaleDefault] = useState(
    initial?.rationale?.default ?? "",
  );
  const [kind, setKind] = useState<RuleKind>(
    initial?.kind ?? { kind: "node_shape", target_node_type_id: "" },
  );
  const [severity, setSeverity] = useState<Severity>(
    initial?.severity ?? "violation",
  );
  const [enforcement, setEnforcement] = useState<EnforcementKind>(
    initial?.enforcement ?? "write",
  );
  const [activation, setActivation] = useState<RuleActivationKind>(
    initial?.activation ?? { kind: "always" },
  );
  const [constraints, setConstraints] = useState<ShaclConstraint[]>(
    initial?.constraints ?? [],
  );

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (isDerived) return;
    onSubmit({
      id: id.trim(),
      name: {
        default: nameDefault.trim(),
        translations: initial?.name?.translations ?? {},
      },
      description: descDefault
        ? {
            default: descDefault,
            translations: initial?.description?.translations ?? {},
          }
        : undefined,
      rationale: rationaleDefault
        ? {
            default: rationaleDefault,
            translations: initial?.rationale?.translations ?? {},
          }
        : undefined,
      kind,
      severity,
      enforcement,
      activation,
      origin: initial?.origin ?? { kind: "authored" },
      constraints,
      valid_from: initial?.valid_from ?? null,
      valid_to: initial?.valid_to ?? null,
    });
  };

  const handleKindChange = (nextKind: RuleKind["kind"]) => {
    switch (nextKind) {
      case "node_shape":
        setKind({ kind: "node_shape", target_node_type_id: "" });
        break;
      case "property_shape":
        setKind({
          kind: "property_shape",
          target_node_type_id: "",
          target_property_id: "",
        });
        break;
      case "edge_shape":
        setKind({ kind: "edge_shape", target_edge_type_id: "" });
        break;
      case "cross_entity_shape":
        setKind({ kind: "cross_entity_shape", predicate: "" });
        break;
      case "state_machine":
        setKind({
          kind: "state_machine",
          target_node_type_id: "",
          state_property_id: "",
          transitions: [],
        });
        break;
    }
  };

  const handleActivationChange = (next: RuleActivationKind["kind"]) => {
    switch (next) {
      case "always":
        setActivation({ kind: "always" });
        break;
      case "on_action":
        setActivation({ kind: "on_action", action_id: "" });
        break;
      case "on_schedule":
        setActivation({ kind: "on_schedule", cron_expression: "" });
        break;
    }
  };

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3">
      {isDerived && (
        <p className="rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-300">
          {t("derivedNotice")}
        </p>
      )}

      <SettingsInput
        label={t("id")}
        value={id}
        onChange={(e) => setId(e.target.value)}
        placeholder="rule-min-email"
        required
        disabled={isDerived || !!initial}
      />
      <SettingsInput
        label={t("name")}
        value={nameDefault}
        onChange={(e) => setNameDefault(e.target.value)}
        required
        disabled={isDerived}
      />
      <SettingsTextarea
        label={t("description")}
        value={descDefault}
        onChange={(e) => setDescDefault(e.target.value)}
        rows={2}
        disabled={isDerived}
      />
      <SettingsTextarea
        label={t("rationale")}
        value={rationaleDefault}
        onChange={(e) => setRationaleDefault(e.target.value)}
        rows={3}
        placeholder={t("rationalePlaceholder")}
        disabled={isDerived}
      />

      <div className="grid grid-cols-3 gap-2">
        <EnumPicker
          label={t("severity")}
          value={severity}
          options={SEVERITY_OPTIONS}
          onChange={(v) => setSeverity(v as Severity)}
          translationNs="settings.vocabulary.rules.severity"
          disabled={isDerived}
        />
        <EnumPicker
          label={t("enforcement")}
          value={enforcement}
          options={ENFORCEMENT_OPTIONS}
          onChange={(v) => setEnforcement(v as EnforcementKind)}
          translationNs="settings.vocabulary.rules.enforcement"
          disabled={isDerived}
        />
        <EnumPicker
          label={t("activation")}
          value={activation.kind}
          options={ACTIVATION_OPTIONS}
          onChange={(v) =>
            handleActivationChange(v as RuleActivationKind["kind"])
          }
          translationNs="settings.vocabulary.rules.activation"
          disabled={isDerived}
        />
      </div>

      {activation.kind === "on_action" && (
        <SettingsInput
          label={t("activationActionId")}
          value={activation.action_id}
          onChange={(e) =>
            setActivation({ ...activation, action_id: e.target.value })
          }
          required
          disabled={isDerived}
        />
      )}
      {activation.kind === "on_schedule" && (
        <SettingsInput
          label={t("activationCron")}
          value={activation.cron_expression}
          onChange={(e) =>
            setActivation({ ...activation, cron_expression: e.target.value })
          }
          placeholder="0 0 * * *"
          required
          disabled={isDerived}
        />
      )}

      <fieldset className="rounded border border-zinc-200 p-2 dark:border-zinc-700">
        <legend className="px-1 text-[10px] font-medium uppercase tracking-wide text-zinc-500 dark:text-zinc-400">
          {t("kindFieldset")}
        </legend>
        <EnumPicker
          label={t("kind")}
          value={kind.kind}
          options={RULE_KIND_OPTIONS}
          onChange={(v) => handleKindChange(v as RuleKind["kind"])}
          translationNs="settings.vocabulary.rules.kinds"
          disabled={isDerived}
        />
        <RuleKindFields kind={kind} onChange={setKind} disabled={isDerived} />
      </fieldset>

      <fieldset className="rounded border border-zinc-200 p-2 dark:border-zinc-700">
        <legend className="px-1 text-[10px] font-medium uppercase tracking-wide text-zinc-500 dark:text-zinc-400">
          {t("constraintsFieldset")}
        </legend>
        <div className="flex flex-col gap-2">
          {constraints.map((c, idx) => (
            <ConstraintForm
              key={idx}
              value={c}
              onChange={(next) =>
                setConstraints((prev) => prev.map((p, i) => (i === idx ? next : p)))
              }
              onRemove={() =>
                setConstraints((prev) => prev.filter((_, i) => i !== idx))
              }
            />
          ))}
          {!isDerived && (
            <AddConstraintMenu
              onAdd={(c) => setConstraints((prev) => [...prev, c])}
            />
          )}
        </div>
      </fieldset>

      <div className="mt-1 flex items-center justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={pending}
        >
          {t("cancel")}
        </Button>
        <Button
          type="submit"
          size="sm"
          disabled={
            pending || isDerived || !id.trim() || !nameDefault.trim()
          }
        >
          {initial ? t("submitUpdate") : t("submitCreate")}
        </Button>
      </div>
    </form>
  );
}

interface EnumPickerProps {
  label: string;
  value: string;
  options: readonly string[];
  onChange: (next: string) => void;
  translationNs: string;
  disabled?: boolean;
}

function EnumPicker({
  label,
  value,
  options,
  onChange,
  translationNs,
  disabled,
}: EnumPickerProps) {
  const t = useTranslations(translationNs);
  return (
    <label className="flex flex-col gap-1 text-xs text-zinc-600 dark:text-zinc-300">
      <span className="font-medium">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className="rounded border border-zinc-200 bg-white px-2 py-1 disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-900"
      >
        {options.map((opt) => (
          <option key={opt} value={opt}>
            {t(opt)}
          </option>
        ))}
      </select>
    </label>
  );
}

interface RuleKindFieldsProps {
  kind: RuleKind;
  onChange: (next: RuleKind) => void;
  disabled?: boolean;
}

function RuleKindFields({ kind, onChange, disabled }: RuleKindFieldsProps) {
  const t = useTranslations("settings.vocabulary.rules.kindFields");
  switch (kind.kind) {
    case "node_shape":
      return (
        <SettingsInput
          label={t("targetNodeTypeId")}
          value={kind.target_node_type_id}
          onChange={(e) =>
            onChange({ ...kind, target_node_type_id: e.target.value })
          }
          disabled={disabled}
          required
        />
      );
    case "property_shape":
      return (
        <div className="grid grid-cols-2 gap-2">
          <SettingsInput
            label={t("targetNodeTypeId")}
            value={kind.target_node_type_id}
            onChange={(e) =>
              onChange({ ...kind, target_node_type_id: e.target.value })
            }
            disabled={disabled}
            required
          />
          <SettingsInput
            label={t("targetPropertyId")}
            value={kind.target_property_id}
            onChange={(e) =>
              onChange({ ...kind, target_property_id: e.target.value })
            }
            disabled={disabled}
            required
          />
        </div>
      );
    case "edge_shape":
      return (
        <SettingsInput
          label={t("targetEdgeTypeId")}
          value={kind.target_edge_type_id}
          onChange={(e) =>
            onChange({ ...kind, target_edge_type_id: e.target.value })
          }
          disabled={disabled}
          required
        />
      );
    case "cross_entity_shape":
      return (
        <SettingsTextarea
          label={t("predicate")}
          value={kind.predicate}
          onChange={(e) => onChange({ ...kind, predicate: e.target.value })}
          rows={3}
          disabled={disabled}
          required
        />
      );
    case "state_machine":
      return (
        <div className="grid grid-cols-2 gap-2">
          <SettingsInput
            label={t("targetNodeTypeId")}
            value={kind.target_node_type_id}
            onChange={(e) =>
              onChange({ ...kind, target_node_type_id: e.target.value })
            }
            disabled={disabled}
            required
          />
          <SettingsInput
            label={t("statePropertyId")}
            value={kind.state_property_id}
            onChange={(e) =>
              onChange({ ...kind, state_property_id: e.target.value })
            }
            disabled={disabled}
            required
          />
        </div>
      );
  }
}

"use client";

import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";

import { useDraftPersistence } from "@/hooks/use-draft-persistence";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";
import { snapshotEqual } from "@/lib/snapshot-equal";
import { useTranslations } from "next-intl";
import { z } from "zod";

import {
  FormSelect,
  SettingsInput,
  SettingsTextarea,
} from "@/components/ui/form-input";
import { SaveBar } from "@/components/ui/save-bar";
import type {
  EnforcementKind,
  RuleActivationKind,
  RuleDef,
  RuleKind,
  ShaclConstraint,
  Severity,
} from "@/lib/api/edit-ops";
import { IntegrityIssuesBanner } from "@/components/ontology/integrity-issues-banner";
import {
  diagnosticHasParam,
  useOntologyValidation,
} from "@/hooks/api/use-ontology-validation";

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

interface RuleFormDraft {
  id: string;
  nameDefault: string;
  descDefault: string;
  rationaleDefault: string;
  kind: RuleKind;
  severity: Severity;
  enforcement: EnforcementKind;
  activation: RuleActivationKind;
  constraints: ShaclConstraint[];
}

// Schema validates the only two free-text fields the UX is gated on:
// the id (slug-shaped to fit downstream URL routing + IR lookup
// keys) and the localised display name (must be non-empty in the
// canonical locale). Every other field — kind, severity, enforce-
// ment, activation — is a typed enum the picker UI cannot produce
// an invalid value for, so the schema treats them as already-valid.
//
// Error messages are i18n keys; the form translates them at render
// time so the schema definition stays free of localisation.
const RULE_FORM_SCHEMA = z.object({
  id: z
    .string()
    .trim()
    .min(1, { message: "errors.idRequired" })
    .regex(/^[a-z][a-z0-9-_]*$/, { message: "errors.idFormat" }),
  nameDefault: z.string().trim().min(1, { message: "errors.nameRequired" }),
});

type RuleFormSchemaInput = z.input<typeof RULE_FORM_SCHEMA>;

interface RuleFormProps {
  /** Initial rule when editing; `undefined` produces a blank
   *  create form. */
  initial?: RuleDef;
  /** Ontology the rule belongs to. When provided, the form fetches
   *  the IR's referential-integrity diagnostics and surfaces any
   *  that mention this rule above the action row. Omit on create
   *  flows where the rule does not yet exist in the persisted IR. */
  ontologyId?: string | null;
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
  ontologyId,
  onSubmit,
  onCancel,
  pending = false,
}: RuleFormProps) {
  const t = useTranslations("settings.vocabulary.rules.form");
  const isDerived = initial?.origin?.kind === "derived_from_binding";
  const validation = useOntologyValidation(initial ? ontologyId : null);
  const ruleIssues = (validation.data ?? []).filter((d) =>
    initial ? diagnosticHasParam(d, "rule_id", initial.id) : false,
  );

  // Drafts are scoped to the create flow only. Editing an existing
  // rule is server-of-truth; surfacing a stale draft over a fresh
  // server snapshot would confuse the user, so the draft layer
  // sits this one out. The key includes `:new` so concurrent create
  // tabs don't fight — multi-tab create flows are rare enough that
  // last-writer-wins on the same key is the right ergonomic.
  const isCreate = !initial && !isDerived;
  const {
    draft: draftValue,
    hasDraft: hasDraftSnapshot,
    save: saveDraft,
    clear: clearDraft,
  } = useDraftPersistence<RuleFormDraft>({
    key: "draft:rule:new",
  });

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
  const [draftBannerOpen, setDraftBannerOpen] = useState(
    isCreate && hasDraftSnapshot,
  );

  // Snapshot of every editable slot. Drives both auto-save (draft
  // persistence) and the SaveBar dirty calculation.
  const currentSnapshot = useMemo<RuleFormDraft>(
    () => ({
      id,
      nameDefault,
      descDefault,
      rationaleDefault,
      kind,
      severity,
      enforcement,
      activation,
      constraints,
    }),
    [
      id,
      nameDefault,
      descDefault,
      rationaleDefault,
      kind,
      severity,
      enforcement,
      activation,
      constraints,
    ],
  );

  // Initial-form snapshot — what the rule looked like when the form
  // mounted. Computed once per `initial` so the SaveBar dirty flag
  // can deep-compare against it without re-deriving.
  const initialSnapshot = useMemo<RuleFormDraft>(
    () => ({
      id: initial?.id ?? "",
      nameDefault: initial?.name?.default ?? "",
      descDefault: initial?.description?.default ?? "",
      rationaleDefault: initial?.rationale?.default ?? "",
      kind: initial?.kind ?? { kind: "node_shape", target_node_type_id: "" },
      severity: initial?.severity ?? "violation",
      enforcement: initial?.enforcement ?? "write",
      activation: initial?.activation ?? { kind: "always" },
      constraints: initial?.constraints ?? [],
    }),
    [initial],
  );

  const dirty = useMemo(
    () => !snapshotEqual(currentSnapshot, initialSnapshot),
    [currentSnapshot, initialSnapshot],
  );

  // Persist every form-state change (debounced inside the hook).
  useEffect(() => {
    if (!isCreate) return;
    saveDraft(currentSnapshot);
  }, [isCreate, currentSnapshot, saveDraft]);

  const restoreDraft = useCallback(() => {
    if (!draftValue) return;
    setId(draftValue.id);
    setNameDefault(draftValue.nameDefault);
    setDescDefault(draftValue.descDefault);
    setRationaleDefault(draftValue.rationaleDefault);
    setKind(draftValue.kind);
    setSeverity(draftValue.severity);
    setEnforcement(draftValue.enforcement);
    setActivation(draftValue.activation);
    setConstraints(draftValue.constraints);
    setDraftBannerOpen(false);
  }, [draftValue]);

  const dismissDraft = useCallback(() => {
    clearDraft();
    setDraftBannerOpen(false);
  }, [clearDraft]);

  const { errors, submit, clearErrors } = useFormWithSchema({
    schema: RULE_FORM_SCHEMA,
    onValid: ({ id: validId, nameDefault: validName }) => {
      clearDraft();
      onSubmit({
        id: validId,
        name: {
          default: validName,
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
        valid_from: initial?.valid_from,
        valid_to: initial?.valid_to,
      });
    },
  });

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (isDerived) return;
    void submit({ id, nameDefault } satisfies RuleFormSchemaInput);
  };

  const idError = errors.id ? t(errors.id) : undefined;
  const nameError = errors.nameDefault ? t(errors.nameDefault) : undefined;

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
        <p className="rounded border border-warning-border bg-warning-surface p-2 text-xs text-warning-foreground/30">
          {t("derivedNotice")}
        </p>
      )}

      {draftBannerOpen && (
        <div className="flex items-center gap-2 rounded-md border border-info-border bg-info-surface px-3 py-2 text-xs">
          <span className="flex-1 text-info-foreground">{t("draftFound")}</span>
          <button
            type="button"
            onClick={restoreDraft}
            className="rounded-md border border-info-border bg-surface-base px-2 py-1 text-info-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-info-foreground/40"
          >
            {t("draftRestore")}
          </button>
          <button
            type="button"
            onClick={dismissDraft}
            className="rounded-md px-2 py-1 text-info-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-base focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-info-foreground/40"
          >
            {t("draftDiscard")}
          </button>
        </div>
      )}

      <div>
        <SettingsInput
          label={t("id")}
          value={id}
          onChange={(e) => {
            setId(e.target.value);
            clearErrors("id");
          }}
          // i18n-audit-ignore — rule-id slug example, language-neutral identifier
          placeholder="rule-min-email"
          required
          disabled={isDerived || !!initial}
          error={!!idError}
          aria-describedby={idError ? "rule-form-id-error" : undefined}
        />
        {idError && (
          <p
            id="rule-form-id-error"
            role="alert"
            className="mt-1 text-2xs text-danger-foreground"
          >
            {idError}
          </p>
        )}
      </div>
      <div>
        <SettingsInput
          label={t("name")}
          value={nameDefault}
          onChange={(e) => {
            setNameDefault(e.target.value);
            clearErrors("nameDefault");
          }}
          required
          disabled={isDerived}
          error={!!nameError}
          aria-describedby={nameError ? "rule-form-name-error" : undefined}
        />
        {nameError && (
          <p
            id="rule-form-name-error"
            role="alert"
            className="mt-1 text-2xs text-danger-foreground"
          >
            {nameError}
          </p>
        )}
      </div>
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

      <fieldset className="rounded border border-divider p-2">
        <legend className="px-1 text-2xs font-medium uppercase tracking-wide text-foreground-muted">
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

      <fieldset className="rounded border border-divider p-2">
        <legend className="px-1 text-2xs font-medium uppercase tracking-wide text-foreground-muted">
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

      <IntegrityIssuesBanner issues={ruleIssues} />

      <SaveBar
        dirty={dirty && !isDerived}
        pending={pending}
        onSave={() => {
          void submit({ id, nameDefault } satisfies RuleFormSchemaInput);
        }}
        onDiscard={onCancel}
      />
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
    <label className="flex flex-col gap-1 text-xs text-foreground-muted">
      <span className="font-medium">{label}</span>
      <FormSelect
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        density="compact"
      >
        {options.map((opt) => (
          <option key={opt} value={opt}>
            {t(opt)}
          </option>
        ))}
      </FormSelect>
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

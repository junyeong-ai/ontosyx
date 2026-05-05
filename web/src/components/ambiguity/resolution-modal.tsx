"use client";

// Resolution editor. Three mapping modes — ValueMap, CodeSystemRef,
// GlossaryRef — each maps 1:1 onto the Rust `AmbiguityMapping`
// variant the backend persists.
//
// The modal is single-purpose (one context at a time) and stateless
// about persistence — the parent fires `useResolveAmbiguity` with
// the finalised payload.

import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { FormField } from "@/components/ui/form-field";
import { FormInput } from "@/components/ui/form-input";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";
import type {
  AmbiguityContext,
  AmbiguityMapping,
  AmbiguityResolution,
} from "@/lib/api/ambiguity";

import {
  AmbiguityMappingFormSchema,
  toAmbiguityMapping,
  type ValidatedAmbiguityMapping,
} from "./ambiguity-mapping-schema";

type Mode = "value_map" | "code_system_ref" | "glossary_ref";

export interface ResolutionModalProps {
  context: AmbiguityContext;
  /** Current active resolution, if any. Pre-fills the form so an
   * admin editing an existing resolution starts from the last
   * state instead of a blank slate. */
  active?: AmbiguityResolution | null;
  onSubmit: (mapping: AmbiguityMapping) => void;
  onCancel: () => void;
  busy?: boolean;
}

export function ResolutionModal({
  context,
  active,
  onSubmit,
  onCancel,
  busy,
}: ResolutionModalProps) {
  const t = useTranslations("settings.quality.ambiguity.modal");

  // Derive the initial mode from the active resolution's mapping, if
  // any; default to ValueMap (the most common / flexible shape).
  const initialMode: Mode =
    active?.mapping.kind === "code_system_ref"
      ? "code_system_ref"
      : active?.mapping.kind === "glossary_ref"
        ? "glossary_ref"
        : "value_map";

  const [mode, setMode] = useState<Mode>(initialMode);

  // ValueMap entries — seed from the active mapping if in that mode,
  // otherwise seed one row per sample value so the admin just types
  // labels.
  const initialEntries = useMemo(() => {
    if (active?.mapping.kind === "value_map") {
      return active.mapping.entries.map((e) => ({
        value: e.value,
        display: e.display,
        definition: e.definition ?? "",
      }));
    }
    return (context.sample_values ?? []).map((v) => ({
      value: v,
      display: "",
      definition: "",
    }));
  }, [active, context.sample_values]);

  const [entries, setEntries] =
    useState<Array<{ value: string; display: string; definition: string }>>(
      initialEntries,
    );
  const [codeSystemId, setCodeSystemId] = useState(
    active?.mapping.kind === "code_system_ref"
      ? active.mapping.code_system_id
      : "",
  );
  const [termId, setTermId] = useState(
    active?.mapping.kind === "glossary_ref" ? active.mapping.term_id : "",
  );

  // Re-seed entries when the underlying context changes (pin id).
  useEffect(() => {
    setEntries(initialEntries);
  }, [initialEntries]);

  const onValid = useCallback(
    (validated: ValidatedAmbiguityMapping) => {
      onSubmit(toAmbiguityMapping(validated));
    },
    [onSubmit],
  );

  const { errors, submit, clearErrors } = useFormWithSchema({
    schema: AmbiguityMappingFormSchema,
    onValid,
  });

  const handleSubmit = () => {
    if (mode === "value_map") {
      void submit({ kind: "value_map", entries });
      return;
    }
    if (mode === "code_system_ref") {
      void submit({ kind: "code_system_ref", code_system_id: codeSystemId });
      return;
    }
    void submit({ kind: "glossary_ref", term_id: termId });
  };

  const localizeError = (key: string | undefined) =>
    key ? t(key) : undefined;
  const valueMapError = localizeError(errors.entries ?? errors._form);
  const codeSystemError = localizeError(errors.code_system_id);
  const termError = localizeError(errors.term_id);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="ambiguity-modal-title"
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="fixed inset-0 z-modal flex items-center justify-center bg-[var(--color-surface-overlay)] p-4 backdrop-blur-sm"
    >
      <div className="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-divider bg-surface-base shadow-4">
        {/* Header */}
        <header className="border-b border-divider px-5 py-3">
          <h2
            id="ambiguity-modal-title"
            className="text-sm font-semibold text-foreground-strong"
          >
            {t("title", {
              relation: context.column.relation,
              column: context.column.column,
            })}
          </h2>
          <p className="mt-1 text-xs text-foreground-muted">
            {context.clarification_prompt}
          </p>
        </header>

        {/* Body */}
        <div className="flex-1 overflow-auto px-5 py-4 text-xs">
          {/* Mode switcher */}
          <fieldset className="mb-4 flex gap-2" aria-label={t("modeLabel")}>
            {(["value_map", "code_system_ref", "glossary_ref"] as const).map((m) => (
              <button
                key={m}
                type="button"
                role="radio"
                aria-checked={mode === m}
                onClick={() => {
                  setMode(m);
                  clearErrors();
                }}
                className={`cursor-pointer rounded border px-3 py-1.5 text-start ${
                  mode === m
                    ? "border-concept-foreground bg-concept-surface text-concept-foreground"
                    : "border-divider bg-surface-base text-foreground-muted hover:bg-surface-raised"
                }`}
              >
                {t(`mode.${m}`)}
              </button>
            ))}
          </fieldset>

          {mode === "value_map" && (
            <>
              <ValueMapEditor
                entries={entries}
                onChange={(next) => {
                  setEntries(next);
                  clearErrors();
                }}
              />
              {valueMapError && (
                <p
                  role="alert"
                  className="mt-2 text-2xs text-danger-foreground"
                >
                  {valueMapError}
                </p>
              )}
            </>
          )}
          {mode === "code_system_ref" && (
            <FormField
              label={t("codeSystemIdLabel")}
              error={codeSystemError}
              hint={t("codeSystemHint")}
            >
              <FormInput
                value={codeSystemId}
                onChange={(e) => {
                  setCodeSystemId(e.target.value);
                  clearErrors("code_system_id");
                }}
                // i18n-audit-ignore — code-system slug example, language-neutral identifier
                placeholder="cs-order-status"
                error={!!codeSystemError}
              />
            </FormField>
          )}
          {mode === "glossary_ref" && (
            <FormField
              label={t("termIdLabel")}
              error={termError}
              hint={t("termHint")}
            >
              <FormInput
                value={termId}
                onChange={(e) => {
                  setTermId(e.target.value);
                  clearErrors("term_id");
                }}
                // i18n-audit-ignore — glossary-term slug example, language-neutral identifier
                placeholder="g-vip-tier"
                error={!!termError}
              />
            </FormField>
          )}

          {context.repo_hint && (
            <aside className="mt-4 rounded border border-warning-border bg-warning-surface p-2 text-2xs">
              <p className="font-medium text-warning-foreground">
                {t("repoHintLabel")}
              </p>
              <p className="mt-0.5 text-foreground-muted">
                {context.repo_hint.source_file}
              </p>
              <pre className="mt-1 whitespace-pre-wrap font-mono text-2xs">
                {context.repo_hint.suggested_values}
              </pre>
            </aside>
          )}
        </div>

        {/* Footer */}
        <footer className="flex items-center justify-end gap-2 border-t border-divider px-5 py-3">
          <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
            {t("cancel")}
          </Button>
          <Button
            type="button"
            variant="primary"
            size="sm"
            onClick={handleSubmit}
            loading={busy}
          >
            {t("save")}
          </Button>
        </footer>
      </div>
    </div>
  );
}

function ValueMapEditor({
  entries,
  onChange,
}: {
  entries: Array<{ value: string; display: string; definition: string }>;
  onChange: (next: typeof entries) => void;
}) {
  const t = useTranslations("settings.quality.ambiguity.modal");
  return (
    <div className="space-y-1.5">
      <div className="grid grid-cols-[1fr_1fr_2fr_24px] gap-2 text-2xs font-medium text-foreground-muted">
        <span>{t("valueHeader")}</span>
        <span>{t("displayHeader")}</span>
        <span>{t("definitionHeader")}</span>
        <span></span>
      </div>
      {entries.map((e, idx) => (
        <div
          key={idx}
          className="grid grid-cols-[1fr_1fr_2fr_24px] gap-2"
        >
          <FormInput
            aria-label={t("valueInput", { index: idx + 1 })}
            value={e.value}
            onChange={(ev) => {
              const next = entries.slice();
              next[idx] = { ...e, value: ev.target.value };
              onChange(next);
            }}
            className="px-1.5 py-1"
          />
          <FormInput
            aria-label={t("displayInput", { index: idx + 1 })}
            value={e.display}
            onChange={(ev) => {
              const next = entries.slice();
              next[idx] = { ...e, display: ev.target.value };
              onChange(next);
            }}
            className="px-1.5 py-1"
          />
          <FormInput
            aria-label={t("definitionInput", { index: idx + 1 })}
            value={e.definition}
            onChange={(ev) => {
              const next = entries.slice();
              next[idx] = { ...e, definition: ev.target.value };
              onChange(next);
            }}
            className="px-1.5 py-1"
          />
          <button
            type="button"
            onClick={() => onChange(entries.filter((_, i) => i !== idx))}
            aria-label={t("removeRow", { index: idx + 1 })}
            className="rounded text-foreground-muted hover:text-danger-foreground"
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={() =>
          onChange([...entries, { value: "", display: "", definition: "" }])
        }
        className="text-2xs text-concept-foreground hover:underline"
      >
        + {t("addRow")}
      </button>
    </div>
  );
}

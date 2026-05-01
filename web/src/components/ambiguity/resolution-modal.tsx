"use client";

// Resolution editor. Three mapping modes — ValueMap, CodeSystemRef,
// GlossaryRef — each maps 1:1 onto the Rust `AmbiguityMapping`
// variant the backend persists.
//
// The modal is single-purpose (one context at a time) and stateless
// about persistence — the parent fires `useResolveAmbiguity` with
// the finalised payload.

import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import type {
  AmbiguityContext,
  AmbiguityMapping,
  AmbiguityResolution,
} from "@/lib/api/ambiguity";

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
  const t = useTranslations("settings.ambiguity.modal");

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

  const handleSubmit = () => {
    if (mode === "value_map") {
      const filtered = entries.filter(
        (e) => e.value.trim() !== "" && e.display.trim() !== "",
      );
      if (filtered.length === 0) {
        toast.error(t("errors.valueMapEmpty"));
        return;
      }
      onSubmit({
        kind: "value_map",
        entries: filtered.map((e) => ({
          value: e.value,
          display: e.display,
          definition: e.definition.trim() ? e.definition : undefined,
        })),
      });
      return;
    }
    if (mode === "code_system_ref") {
      if (!codeSystemId.trim()) {
        toast.error(t("errors.codeSystemRequired"));
        return;
      }
      onSubmit({ kind: "code_system_ref", code_system_id: codeSystemId.trim() });
      return;
    }
    if (!termId.trim()) {
      toast.error(t("errors.termRequired"));
      return;
    }
    onSubmit({ kind: "glossary_ref", term_id: termId.trim() });
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="ambiguity-modal-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-900/40 p-4 backdrop-blur-sm"
    >
      <div className="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-950">
        {/* Header */}
        <header className="border-b border-zinc-200 px-5 py-3 dark:border-zinc-800">
          <h2
            id="ambiguity-modal-title"
            className="text-sm font-semibold text-zinc-900 dark:text-zinc-100"
          >
            {t("title", {
              relation: context.column.relation,
              column: context.column.column,
            })}
          </h2>
          <p className="mt-1 text-xs text-muted-foreground">
            {context.clarification_prompt}
          </p>
        </header>

        {/* Body */}
        <div className="flex-1 overflow-auto px-5 py-4 text-xs">
          {/* Mode switcher */}
          <fieldset className="mb-4 flex gap-2" aria-label={t("modeLabel")}>
            {(["value_map", "code_system_ref", "glossary_ref"] as const).map((m) => (
              <label
                key={m}
                className={`cursor-pointer rounded border px-3 py-1.5 ${
                  mode === m
                    ? "border-violet-500 bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                    : "border-zinc-200 bg-white text-muted-foreground hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:hover:bg-zinc-800"
                }`}
              >
                <input
                  type="radio"
                  name="mode"
                  value={m}
                  checked={mode === m}
                  onChange={() => setMode(m)}
                  className="sr-only"
                />
                {t(`mode.${m}`)}
              </label>
            ))}
          </fieldset>

          {mode === "value_map" && (
            <ValueMapEditor entries={entries} onChange={setEntries} />
          )}
          {mode === "code_system_ref" && (
            <div className="space-y-1">
              <label
                htmlFor="code-system-id"
                className="block text-[11px] font-medium text-zinc-700 dark:text-zinc-300"
              >
                {t("codeSystemIdLabel")}
              </label>
              <input
                id="code-system-id"
                value={codeSystemId}
                onChange={(e) => setCodeSystemId(e.target.value)}
                placeholder="cs-order-status"
                className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 text-xs dark:border-zinc-600 dark:bg-zinc-900"
              />
              <p className="mt-1 text-[10px] text-muted-foreground">
                {t("codeSystemHint")}
              </p>
            </div>
          )}
          {mode === "glossary_ref" && (
            <div className="space-y-1">
              <label
                htmlFor="term-id"
                className="block text-[11px] font-medium text-zinc-700 dark:text-zinc-300"
              >
                {t("termIdLabel")}
              </label>
              <input
                id="term-id"
                value={termId}
                onChange={(e) => setTermId(e.target.value)}
                placeholder="g-vip-tier"
                className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 text-xs dark:border-zinc-600 dark:bg-zinc-900"
              />
              <p className="mt-1 text-[10px] text-muted-foreground">
                {t("termHint")}
              </p>
            </div>
          )}

          {context.repo_hint && (
            <aside className="mt-4 rounded border border-amber-200 bg-amber-50 p-2 text-[11px] dark:border-amber-900 dark:bg-amber-950/30">
              <p className="font-medium text-amber-700 dark:text-amber-400">
                {t("repoHintLabel")}
              </p>
              <p className="mt-0.5 text-muted-foreground">
                {context.repo_hint.source_file}
              </p>
              <pre className="mt-1 whitespace-pre-wrap font-mono text-[10px]">
                {context.repo_hint.suggested_values}
              </pre>
            </aside>
          )}
        </div>

        {/* Footer */}
        <footer className="flex items-center justify-end gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800">
          <button
            type="button"
            onClick={onCancel}
            className="rounded px-3 py-1.5 text-xs text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            {t("cancel")}
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={busy}
            className="rounded bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-700 disabled:opacity-50"
          >
            {busy ? t("saving") : t("save")}
          </button>
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
  const t = useTranslations("settings.ambiguity.modal");
  return (
    <div className="space-y-1.5">
      <div className="grid grid-cols-[1fr_1fr_2fr_24px] gap-2 text-[10px] font-medium text-muted-foreground">
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
          <input
            aria-label={t("valueInput", { index: idx + 1 })}
            value={e.value}
            onChange={(ev) => {
              const next = entries.slice();
              next[idx] = { ...e, value: ev.target.value };
              onChange(next);
            }}
            className="rounded border border-zinc-300 bg-white px-1.5 py-1 dark:border-zinc-600 dark:bg-zinc-900"
          />
          <input
            aria-label={t("displayInput", { index: idx + 1 })}
            value={e.display}
            onChange={(ev) => {
              const next = entries.slice();
              next[idx] = { ...e, display: ev.target.value };
              onChange(next);
            }}
            className="rounded border border-zinc-300 bg-white px-1.5 py-1 dark:border-zinc-600 dark:bg-zinc-900"
          />
          <input
            aria-label={t("definitionInput", { index: idx + 1 })}
            value={e.definition}
            onChange={(ev) => {
              const next = entries.slice();
              next[idx] = { ...e, definition: ev.target.value };
              onChange(next);
            }}
            className="rounded border border-zinc-300 bg-white px-1.5 py-1 dark:border-zinc-600 dark:bg-zinc-900"
          />
          <button
            type="button"
            onClick={() => onChange(entries.filter((_, i) => i !== idx))}
            aria-label={t("removeRow", { index: idx + 1 })}
            className="rounded text-muted-foreground hover:text-rose-500"
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
        className="text-[10px] text-violet-600 hover:underline dark:text-violet-400"
      >
        + {t("addRow")}
      </button>
    </div>
  );
}

"use client";

// Glossary term form — surfaces the full canonical
// `ox_ontology::glossary::GlossaryTermDef` shape: localised
// term/display_name/description/aliases/examples, lifecycle
// (active / deprecated / retired) with successor pointer, validity
// window, governance trail, and SKOS relations. The form edits the
// `default` slot of every `LocalizedText` field; existing
// translations are preserved verbatim so an admin who edits a
// Korean-localised term doesn't silently drop the English label.
//
// Lifecycle and origin discriminators map cleanly onto the SKOS-XL
// vocabulary (`owl:deprecated`, `dct:isReplacedBy`, `xl:scopeNote`,
// `xl:editorialNote`, `xl:changeNote`), which is what the SKOS
// exporter walks at `crates/ox-compiler/src/export/glossary_skos.rs`.

import { useMemo, useState, type FormEvent } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import {
  SettingsInput,
  SettingsTextarea,
} from "@/components/ui/form-input";
import { RelationsField } from "@/components/settings/vocabulary/relations-field";
import type {
  GlossaryTermDef,
  TermLifecycle,
  TermOrigin,
  TermRelation,
} from "@/lib/api/edit-ops";
import type { LocalizedText } from "@/types/ontology";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/lib/use-locale-chain";

// ---------------------------------------------------------------------------
// LocalizedText helpers — every text field on `GlossaryTermDef` is a
// `LocalizedText { default, translations }`. The form always edits the
// `default` slot and preserves any existing translations untouched, so
// that an admin who only speaks Korean can edit a Korean term without
// erasing its English mirror.
// ---------------------------------------------------------------------------

function mergeDefault(value: string, base?: LocalizedText): LocalizedText {
  return {
    default: value.trim(),
    translations: base?.translations ?? {},
  };
}

function mergeDefaultOptional(
  value: string,
  base?: LocalizedText,
): LocalizedText | undefined {
  return value.trim().length > 0 ? mergeDefault(value, base) : undefined;
}

/** Convert a list-textarea (one entry per line) to `LocalizedText[]`,
 *  keeping the existing `translations` for any entry whose `default`
 *  matches an original — so reordering or editing one entry doesn't
 *  drop translations on the others. */
function linesToLocalized(
  text: string,
  originals: readonly LocalizedText[] = [],
): LocalizedText[] {
  const byDefault = new Map(originals.map((o) => [o.default, o]));
  return text
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map((line) => byDefault.get(line) ?? { default: line, translations: {} });
}

function localizedToLines(items: readonly LocalizedText[] | undefined): string {
  return (items ?? []).map((it) => it.default).join("\n");
}

// ---------------------------------------------------------------------------
// Lifecycle helpers — `TermLifecycle` is a tagged union. The form
// stores `state` as a single radio + the variant fields side-by-side;
// build the wire shape on submit based on the active state.
// ---------------------------------------------------------------------------

type LifecycleState = TermLifecycle["state"];

function lifecycleState(lc: TermLifecycle | undefined): LifecycleState {
  return lc?.state ?? "active";
}

function buildLifecycle(
  state: LifecycleState,
  replacedBy: string,
  deprecatedAt: string,
  retiredAt: string,
): TermLifecycle {
  if (state === "deprecated") {
    return {
      state: "deprecated",
      deprecated_at:
        fromDateInput(deprecatedAt) ?? new Date().toISOString(),
      replaced_by: replacedBy.trim() || null,
    };
  }
  if (state === "retired") {
    return {
      state: "retired",
      retired_at: fromDateInput(retiredAt) ?? new Date().toISOString(),
    };
  }
  return { state: "active" };
}

// ---------------------------------------------------------------------------
// Origin helpers — `TermOrigin` is also a tagged union. Manual /
// derived-from-column / imported-from each carry their own slug-fields.
// ---------------------------------------------------------------------------

type OriginKind = TermOrigin["kind"];

function originKind(org: TermOrigin | undefined): OriginKind {
  return org?.kind ?? "manual";
}

function buildOrigin(
  kind: OriginKind,
  table: string,
  column: string,
  catalog: string,
  externalId: string,
): TermOrigin {
  if (kind === "derived_from_column") {
    return {
      kind: "derived_from_column",
      table: table.trim(),
      column: column.trim(),
    };
  }
  if (kind === "imported_from") {
    return {
      kind: "imported_from",
      catalog: catalog.trim(),
      external_id: externalId.trim() || null,
    };
  }
  return { kind: "manual" };
}

// ---------------------------------------------------------------------------
// Date helpers — backend round-trips ISO 8601 timestamps, but
// `<input type="datetime-local">` wants the local-zoned `YYYY-MM-DDTHH:mm`
// form. Convert in both directions; an unparseable value collapses to
// `undefined` rather than throwing.
// ---------------------------------------------------------------------------

function toDateInput(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(
    d.getDate(),
  )}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fromDateInput(local: string): string | undefined {
  if (!local) return undefined;
  const d = new Date(local);
  if (Number.isNaN(d.getTime())) return undefined;
  return d.toISOString();
}

// ---------------------------------------------------------------------------
// Form
// ---------------------------------------------------------------------------

interface GlossaryFormProps {
  /** Initial values when editing an existing term; `undefined`
   *  produces a blank create form. */
  initial?: GlossaryTermDef;
  /** Other terms in the glossary — fed into the SKOS relation
   *  picker so target ids resolve to human names, and into the
   *  "replaced by" select for deprecated terms. */
  availableTerms?: GlossaryTermDef[];
  /** Called with the canonical-shape `GlossaryTermDef` on submit.
   *  `id` stays blank for create flows; the page mints one. */
  onSubmit: (def: GlossaryTermDef) => void;
  /** Cancel handler — closes the dialog or exits inline edit mode. */
  onCancel: () => void;
  /** Disable the submit button — callers pass the mutation's
   *  `isPending` so the form locks during the round-trip. */
  pending?: boolean;
}

export function GlossaryForm({
  initial,
  availableTerms,
  onSubmit,
  onCancel,
  pending = false,
}: GlossaryFormProps) {
  const t = useTranslations("settings.vocabulary.glossary.form");
  const localeChain = useLocaleChain();

  // Re-mount the form with a different `key` to bind to a new term —
  // we don't sync from `initial` via `useEffect` (per
  // react-hooks/set-state-in-effect).
  const [termDefault, setTermDefault] = useState(initial?.term?.default ?? "");
  const [displayDefault, setDisplayDefault] = useState(
    initial?.display_name?.default ?? "",
  );
  const [descriptionDefault, setDescriptionDefault] = useState(
    initial?.description?.default ?? "",
  );
  const [aliasesText, setAliasesText] = useState(
    localizedToLines(initial?.aliases),
  );
  const [examplesText, setExamplesText] = useState(
    localizedToLines(initial?.examples),
  );
  const [category, setCategory] = useState(initial?.category ?? "");
  const [relations, setRelations] = useState<TermRelation[]>(
    initial?.related_terms ?? [],
  );

  // Lifecycle state
  const [state, setState] = useState<LifecycleState>(
    lifecycleState(initial?.lifecycle),
  );
  const [replacedBy, setReplacedBy] = useState(() => {
    const lc = initial?.lifecycle;
    return lc && lc.state === "deprecated" ? lc.replaced_by ?? "" : "";
  });
  const [deprecatedAt, setDeprecatedAt] = useState(() => {
    const lc = initial?.lifecycle;
    return lc && lc.state === "deprecated"
      ? toDateInput(lc.deprecated_at)
      : "";
  });
  const [retiredAt, setRetiredAt] = useState(() => {
    const lc = initial?.lifecycle;
    return lc && lc.state === "retired" ? toDateInput(lc.retired_at) : "";
  });

  // Validity window
  const [validFrom, setValidFrom] = useState(toDateInput(initial?.valid_from));
  const [validTo, setValidTo] = useState(toDateInput(initial?.valid_to));

  // Governance — collapsible advanced block
  const [governanceOpen, setGovernanceOpen] = useState(false);
  const [origin, setOriginState] = useState<OriginKind>(
    originKind(initial?.governance?.origin),
  );
  const initialOrigin = initial?.governance?.origin;
  const [originTable, setOriginTable] = useState(
    initialOrigin?.kind === "derived_from_column" ? initialOrigin.table : "",
  );
  const [originColumn, setOriginColumn] = useState(
    initialOrigin?.kind === "derived_from_column" ? initialOrigin.column : "",
  );
  const [originCatalog, setOriginCatalog] = useState(
    initialOrigin?.kind === "imported_from" ? initialOrigin.catalog : "",
  );
  const [originExternalId, setOriginExternalId] = useState(
    initialOrigin?.kind === "imported_from"
      ? initialOrigin.external_id ?? ""
      : "",
  );
  const [scopeNotesText, setScopeNotesText] = useState(
    localizedToLines(initial?.governance?.scope_notes),
  );
  const [editorialNotesText, setEditorialNotesText] = useState(
    localizedToLines(initial?.governance?.editorial_notes),
  );

  const replacedByChoices = useMemo(
    () =>
      (availableTerms ?? []).filter((term) => term.id !== initial?.id),
    [availableTerms, initial?.id],
  );

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!termDefault.trim()) return;
    onSubmit({
      id: initial?.id ?? "",
      term: mergeDefault(termDefault, initial?.term),
      display_name: mergeDefaultOptional(displayDefault, initial?.display_name),
      description: mergeDefaultOptional(
        descriptionDefault,
        initial?.description,
      ),
      aliases: linesToLocalized(aliasesText, initial?.aliases),
      examples: linesToLocalized(examplesText, initial?.examples),
      category: category.trim() || undefined,
      related_terms: relations,
      lifecycle: buildLifecycle(state, replacedBy, deprecatedAt, retiredAt),
      valid_from: fromDateInput(validFrom) ?? undefined,
      valid_to: fromDateInput(validTo) ?? undefined,
      governance: {
        ...(initial?.governance ?? {}),
        origin: buildOrigin(
          origin,
          originTable,
          originColumn,
          originCatalog,
          originExternalId,
        ),
        scope_notes: linesToLocalized(
          scopeNotesText,
          initial?.governance?.scope_notes,
        ),
        editorial_notes: linesToLocalized(
          editorialNotesText,
          initial?.governance?.editorial_notes,
        ),
      },
    });
  };

  const submitLabel = initial ? t("submitUpdate") : t("submitCreate");

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3">
      <SettingsInput
        label={t("term")}
        value={termDefault}
        onChange={(e) => setTermDefault(e.target.value)}
        placeholder={t("termPlaceholder")}
        required
      />
      <SettingsInput
        label={t("displayName")}
        value={displayDefault}
        onChange={(e) => setDisplayDefault(e.target.value)}
        placeholder={t("displayNamePlaceholder")}
      />
      <SettingsTextarea
        label={t("description")}
        value={descriptionDefault}
        onChange={(e) => setDescriptionDefault(e.target.value)}
        placeholder={t("descriptionPlaceholder")}
        rows={3}
      />
      <SettingsTextarea
        label={t("aliases")}
        value={aliasesText}
        onChange={(e) => setAliasesText(e.target.value)}
        placeholder={t("aliasesPlaceholder")}
        rows={2}
      />
      <SettingsTextarea
        label={t("examples")}
        value={examplesText}
        onChange={(e) => setExamplesText(e.target.value)}
        placeholder={t("examplesPlaceholder")}
        rows={2}
      />
      <SettingsInput
        label={t("category")}
        value={category}
        onChange={(e) => setCategory(e.target.value)}
        placeholder={t("categoryPlaceholder")}
      />

      <fieldset className="flex flex-col gap-2 rounded border border-zinc-200 p-3 dark:border-zinc-700">
        <legend className="px-1 text-[11px] font-medium text-zinc-700 dark:text-zinc-300">
          {t("lifecycle.legend")}
        </legend>
        <div
          role="radiogroup"
          aria-label={t("lifecycle.legend")}
          className="flex flex-wrap items-center gap-3 text-xs"
        >
          {(["active", "deprecated", "retired"] as const).map((option) => (
            <label
              key={option}
              className="flex items-center gap-1.5 text-zinc-700 dark:text-zinc-300"
            >
              <input
                type="radio"
                name="lifecycle-state"
                value={option}
                checked={state === option}
                onChange={() => setState(option)}
              />
              {t(`lifecycle.${option}`)}
            </label>
          ))}
        </div>
        {state === "deprecated" && (
          <div className="grid gap-2 sm:grid-cols-2">
            <label className="flex flex-col gap-1 text-[11px] text-zinc-700 dark:text-zinc-300">
              {t("lifecycle.replacedBy")}
              <select
                value={replacedBy}
                onChange={(e) => setReplacedBy(e.target.value)}
                className="rounded border border-zinc-300 bg-white px-2 py-1 text-xs dark:border-zinc-600 dark:bg-zinc-900"
              >
                <option value="">{t("lifecycle.replacedByEmpty")}</option>
                {replacedByChoices.map((term) => (
                  <option key={term.id} value={term.id}>
                    {localize(term.term, localeChain)} ({term.id})
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1 text-[11px] text-zinc-700 dark:text-zinc-300">
              {t("lifecycle.deprecatedAt")}
              <input
                type="datetime-local"
                value={deprecatedAt}
                onChange={(e) => setDeprecatedAt(e.target.value)}
                className="rounded border border-zinc-300 bg-white px-2 py-1 text-xs dark:border-zinc-600 dark:bg-zinc-900"
              />
            </label>
          </div>
        )}
        {state === "retired" && (
          <label className="flex flex-col gap-1 text-[11px] text-zinc-700 dark:text-zinc-300">
            {t("lifecycle.retiredAt")}
            <input
              type="datetime-local"
              value={retiredAt}
              onChange={(e) => setRetiredAt(e.target.value)}
              className="rounded border border-zinc-300 bg-white px-2 py-1 text-xs dark:border-zinc-600 dark:bg-zinc-900"
            />
          </label>
        )}
      </fieldset>

      <fieldset className="grid gap-2 rounded border border-zinc-200 p-3 sm:grid-cols-2 dark:border-zinc-700">
        <legend className="px-1 text-[11px] font-medium text-zinc-700 dark:text-zinc-300">
          {t("validity.legend")}
        </legend>
        <label className="flex flex-col gap-1 text-[11px] text-zinc-700 dark:text-zinc-300">
          {t("validity.validFrom")}
          <input
            type="datetime-local"
            value={validFrom}
            onChange={(e) => setValidFrom(e.target.value)}
            className="rounded border border-zinc-300 bg-white px-2 py-1 text-xs dark:border-zinc-600 dark:bg-zinc-900"
          />
        </label>
        <label className="flex flex-col gap-1 text-[11px] text-zinc-700 dark:text-zinc-300">
          {t("validity.validTo")}
          <input
            type="datetime-local"
            value={validTo}
            onChange={(e) => setValidTo(e.target.value)}
            className="rounded border border-zinc-300 bg-white px-2 py-1 text-xs dark:border-zinc-600 dark:bg-zinc-900"
          />
        </label>
      </fieldset>

      <details
        open={governanceOpen}
        onToggle={(e) =>
          setGovernanceOpen((e.target as HTMLDetailsElement).open)
        }
        className="rounded border border-zinc-200 dark:border-zinc-700"
      >
        <summary className="cursor-pointer px-3 py-2 text-[11px] font-medium text-zinc-700 dark:text-zinc-300">
          {t("governance.legend")}
        </summary>
        <div className="flex flex-col gap-2 px-3 pb-3 pt-1">
          <label className="flex flex-col gap-1 text-[11px] text-zinc-700 dark:text-zinc-300">
            {t("governance.originLabel")}
            <select
              value={origin}
              onChange={(e) => setOriginState(e.target.value as OriginKind)}
              className="rounded border border-zinc-300 bg-white px-2 py-1 text-xs dark:border-zinc-600 dark:bg-zinc-900"
            >
              <option value="manual">{t("governance.originManual")}</option>
              <option value="derived_from_column">
                {t("governance.originDerivedFromColumn")}
              </option>
              <option value="imported_from">
                {t("governance.originImportedFrom")}
              </option>
            </select>
          </label>
          {origin === "derived_from_column" && (
            <div className="grid gap-2 sm:grid-cols-2">
              <SettingsInput
                label={t("governance.originTable")}
                value={originTable}
                onChange={(e) => setOriginTable(e.target.value)}
              />
              <SettingsInput
                label={t("governance.originColumn")}
                value={originColumn}
                onChange={(e) => setOriginColumn(e.target.value)}
              />
            </div>
          )}
          {origin === "imported_from" && (
            <div className="grid gap-2 sm:grid-cols-2">
              <SettingsInput
                label={t("governance.originCatalog")}
                value={originCatalog}
                onChange={(e) => setOriginCatalog(e.target.value)}
              />
              <SettingsInput
                label={t("governance.originExternalId")}
                value={originExternalId}
                onChange={(e) => setOriginExternalId(e.target.value)}
              />
            </div>
          )}
          <SettingsTextarea
            label={t("governance.scopeNotes")}
            value={scopeNotesText}
            onChange={(e) => setScopeNotesText(e.target.value)}
            placeholder={t("governance.scopeNotesPlaceholder")}
            rows={2}
          />
          <SettingsTextarea
            label={t("governance.editorialNotes")}
            value={editorialNotesText}
            onChange={(e) => setEditorialNotesText(e.target.value)}
            placeholder={t("governance.editorialNotesPlaceholder")}
            rows={2}
          />
        </div>
      </details>

      <RelationsField
        selfId={initial?.id}
        initial={relations}
        onChange={setRelations}
        availableTerms={availableTerms ?? []}
      />

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
          disabled={pending || !termDefault.trim()}
        >
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}

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

import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { useTranslations } from "next-intl";
import { z } from "zod";

import { useDraftPersistence } from "@/hooks/use-draft-persistence";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";
import { snapshotEqual } from "@/lib/snapshot-equal";
import { ChipInput } from "@/components/ui/chip-input";
import {
  FormInput,
  SettingsInput,
  SettingsSelect,
  SettingsTextarea,
} from "@/components/ui/form-input";
import { SaveBar } from "@/components/ui/save-bar";
import { StatusPill, type StatusPillOption } from "@/components/ui/status-pill";
import { RelationsField } from "@/components/vocabulary/relations-field";
import type {
  GlossaryTermDef,
  TermLifecycle,
  TermOrigin,
  TermRelation,
} from "@/lib/api/edit-ops";
import type { LocalizedText } from "@/types/ontology";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

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

// Chip-input adapter helpers — `LocalizedText` is the canonical
// shape on the wire, but the chip input edits a flat string per
// chip. `mergeChipsWithOriginals` re-attaches the per-entry
// `translations` map for any chip whose `default` matches an
// original entry, so reordering or editing one entry doesn't drop
// translations on the others.
function chipsToLocalized(
  chips: readonly string[],
  originals: readonly LocalizedText[] = [],
): LocalizedText[] {
  const byDefault = new Map(originals.map((o) => [o.default, o]));
  return chips.map(
    (text) => byDefault.get(text) ?? { default: text, translations: {} },
  );
}

function localizedToChips(
  items: readonly LocalizedText[] | undefined,
): string[] {
  return (items ?? []).map((it) => it.default);
}

// ---------------------------------------------------------------------------
// Lifecycle helpers — `TermLifecycle` is a tagged union. The form
// stores `state` as a single radio + the variant fields side-by-side;
// build the wire shape on submit based on the active state.
// ---------------------------------------------------------------------------

type LifecycleState = TermLifecycle["state"];

// Snapshot every editable field on the form into a single object —
// the localStorage draft layer round-trips this verbatim. The shape
// mirrors the in-flight state slots so the restore action becomes
// a fan-out of `setX(draft.x)` calls.
interface GlossaryFormDraft {
  termDefault: string;
  displayDefault: string;
  descriptionDefault: string;
  aliases: string[];
  examples: string[];
  category: string;
  relations: TermRelation[];
  state: LifecycleState;
  replacedBy: string;
  deprecatedAt: string;
  retiredAt: string;
  validFrom: string;
  validTo: string;
  origin: OriginKind;
  originTable: string;
  originColumn: string;
  originCatalog: string;
  originExternalId: string;
  scopeNotes: string[];
  editorialNotes: string[];
}

// Schema validates the only required free-text field — the term
// default. Everything else is optional or typed-enum-driven, so the
// schema treats them as already valid; lifecycle / origin discrim-
// inators carry their structural integrity through the build-on-
// submit functions (`buildLifecycle`, `buildOrigin`) rather than
// the schema layer.
const GLOSSARY_FORM_SCHEMA = z.object({
  termDefault: z.string().trim().min(1, { message: "errors.termRequired" }),
});

type GlossaryFormSchemaInput = z.input<typeof GLOSSARY_FORM_SCHEMA>;

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

  // Drafts are scoped to the create flow only — editing an existing
  // term is server-of-truth, so a stale local draft over fresh data
  // would confuse the user. Same approach as RuleForm. The key is
  // shared across tabs deliberately; concurrent create flows on a
  // single workspace are rare and last-writer-wins is the right
  // ergonomic.
  const isCreate = !initial;
  const {
    draft: draftValue,
    hasDraft: hasDraftSnapshot,
    save: saveDraft,
    clear: clearDraft,
  } = useDraftPersistence<GlossaryFormDraft>({
    key: "draft:glossary:new",
  });

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
  const [aliases, setAliases] = useState<string[]>(
    localizedToChips(initial?.aliases),
  );
  const [examples, setExamples] = useState<string[]>(
    localizedToChips(initial?.examples),
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
  const [scopeNotes, setScopeNotes] = useState<string[]>(
    localizedToChips(initial?.governance?.scope_notes),
  );
  const [editorialNotes, setEditorialNotes] = useState<string[]>(
    localizedToChips(initial?.governance?.editorial_notes),
  );

  const replacedByChoices = useMemo(
    () =>
      (availableTerms ?? []).filter((term) => term.id !== initial?.id),
    [availableTerms, initial?.id],
  );

  // Draft restore banner — shown only when a stored draft exists
  // and we're in create mode. Dismissing or restoring closes it.
  const [draftBannerOpen, setDraftBannerOpen] = useState(
    isCreate && hasDraftSnapshot,
  );

  // Snapshot of every editable slot. Used both for draft persistence
  // (auto-save to localStorage on change) and for the SaveBar dirty
  // calculation (deep-equal against the initial-derived snapshot).
  const currentSnapshot = useMemo<GlossaryFormDraft>(
    () => ({
      termDefault,
      displayDefault,
      descriptionDefault,
      aliases,
      examples,
      category,
      relations,
      state,
      replacedBy,
      deprecatedAt,
      retiredAt,
      validFrom,
      validTo,
      origin,
      originTable,
      originColumn,
      originCatalog,
      originExternalId,
      scopeNotes,
      editorialNotes,
    }),
    [
      termDefault,
      displayDefault,
      descriptionDefault,
      aliases,
      examples,
      category,
      relations,
      state,
      replacedBy,
      deprecatedAt,
      retiredAt,
      validFrom,
      validTo,
      origin,
      originTable,
      originColumn,
      originCatalog,
      originExternalId,
      scopeNotes,
      editorialNotes,
    ],
  );

  // Initial snapshot — what the form looked like when it loaded.
  // Computed once per `initial` so the SaveBar dirty flag can
  // deep-compare against it without re-deriving on every keystroke.
  const initialSnapshot = useMemo<GlossaryFormDraft>(
    () => ({
      termDefault: initial?.term?.default ?? "",
      displayDefault: initial?.display_name?.default ?? "",
      descriptionDefault: initial?.description?.default ?? "",
      aliases: localizedToChips(initial?.aliases),
      examples: localizedToChips(initial?.examples),
      category: initial?.category ?? "",
      relations: initial?.related_terms ?? [],
      state: lifecycleState(initial?.lifecycle),
      replacedBy:
        initial?.lifecycle?.state === "deprecated"
          ? initial.lifecycle.replaced_by ?? ""
          : "",
      deprecatedAt:
        initial?.lifecycle?.state === "deprecated"
          ? toDateInput(initial.lifecycle.deprecated_at)
          : "",
      retiredAt:
        initial?.lifecycle?.state === "retired"
          ? toDateInput(initial.lifecycle.retired_at)
          : "",
      validFrom: toDateInput(initial?.valid_from),
      validTo: toDateInput(initial?.valid_to),
      origin: originKind(initial?.governance?.origin),
      originTable:
        initial?.governance?.origin?.kind === "derived_from_column"
          ? initial.governance.origin.table
          : "",
      originColumn:
        initial?.governance?.origin?.kind === "derived_from_column"
          ? initial.governance.origin.column
          : "",
      originCatalog:
        initial?.governance?.origin?.kind === "imported_from"
          ? initial.governance.origin.catalog
          : "",
      originExternalId:
        initial?.governance?.origin?.kind === "imported_from"
          ? initial.governance.origin.external_id ?? ""
          : "",
      scopeNotes: localizedToChips(initial?.governance?.scope_notes),
      editorialNotes: localizedToChips(initial?.governance?.editorial_notes),
    }),
    [initial],
  );

  const dirty = useMemo(
    () => !snapshotEqual(currentSnapshot, initialSnapshot),
    [currentSnapshot, initialSnapshot],
  );

  // Persist on every state change. The hook debounces internally so
  // a typing burst hits localStorage once after 500ms of inactivity.
  // The dep list is just the memoised snapshot — every slot it
  // captures is already in `currentSnapshot`'s own dep list, so we
  // only re-run when the snapshot identity changes.
  useEffect(() => {
    if (!isCreate) return;
    saveDraft(currentSnapshot);
  }, [isCreate, currentSnapshot, saveDraft]);

  const restoreDraft = useCallback(() => {
    if (!draftValue) return;
    setTermDefault(draftValue.termDefault);
    setDisplayDefault(draftValue.displayDefault);
    setDescriptionDefault(draftValue.descriptionDefault);
    setAliases(draftValue.aliases);
    setExamples(draftValue.examples);
    setCategory(draftValue.category);
    setRelations(draftValue.relations);
    setState(draftValue.state);
    setReplacedBy(draftValue.replacedBy);
    setDeprecatedAt(draftValue.deprecatedAt);
    setRetiredAt(draftValue.retiredAt);
    setValidFrom(draftValue.validFrom);
    setValidTo(draftValue.validTo);
    setOriginState(draftValue.origin);
    setOriginTable(draftValue.originTable);
    setOriginColumn(draftValue.originColumn);
    setOriginCatalog(draftValue.originCatalog);
    setOriginExternalId(draftValue.originExternalId);
    setScopeNotes(draftValue.scopeNotes);
    setEditorialNotes(draftValue.editorialNotes);
    setDraftBannerOpen(false);
  }, [draftValue]);

  const dismissDraft = useCallback(() => {
    clearDraft();
    setDraftBannerOpen(false);
  }, [clearDraft]);

  const { errors, submit, clearErrors } = useFormWithSchema({
    schema: GLOSSARY_FORM_SCHEMA,
    onValid: ({ termDefault: validTerm }) => {
      clearDraft();
      onSubmit({
        id: initial?.id ?? "",
        term: mergeDefault(validTerm, initial?.term),
        display_name: mergeDefaultOptional(displayDefault, initial?.display_name),
        description: mergeDefaultOptional(
          descriptionDefault,
          initial?.description,
        ),
        aliases: chipsToLocalized(aliases, initial?.aliases),
        examples: chipsToLocalized(examples, initial?.examples),
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
          scope_notes: chipsToLocalized(
            scopeNotes,
            initial?.governance?.scope_notes,
          ),
          editorial_notes: chipsToLocalized(
            editorialNotes,
            initial?.governance?.editorial_notes,
          ),
        },
      });
    },
  });

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    void submit({ termDefault } satisfies GlossaryFormSchemaInput);
  };

  const termError = errors.termDefault ? t(errors.termDefault) : undefined;

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3">
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
          label={t("term")}
          value={termDefault}
          onChange={(e) => {
            setTermDefault(e.target.value);
            clearErrors("termDefault");
          }}
          placeholder={t("termPlaceholder")}
          required
          error={!!termError}
          aria-describedby={termError ? "glossary-form-term-error" : undefined}
        />
        {termError && (
          <p
            id="glossary-form-term-error"
            role="alert"
            className="mt-1 text-2xs text-danger-foreground"
          >
            {termError}
          </p>
        )}
      </div>
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
      <div className="flex flex-col gap-1">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("aliases")}
        </span>
        <ChipInput
          values={aliases}
          onChange={setAliases}
          placeholder={t("aliasesPlaceholder")}
        />
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("examples")}
        </span>
        <ChipInput
          values={examples}
          onChange={setExamples}
          placeholder={t("examplesPlaceholder")}
        />
      </div>
      <SettingsInput
        label={t("category")}
        value={category}
        onChange={(e) => setCategory(e.target.value)}
        placeholder={t("categoryPlaceholder")}
      />

      <fieldset className="flex flex-col gap-2 rounded border border-divider p-3">
        <legend className="px-1 text-2xs font-medium text-foreground">
          {t("lifecycle.legend")}
        </legend>
        <div className="flex items-center gap-2">
          <StatusPill<LifecycleState>
            value={state}
            onChange={setState}
            ariaLabel={t("lifecycle.legend")}
            options={
              [
                {
                  key: "active",
                  label: t("lifecycle.active"),
                  tone: "success",
                },
                {
                  key: "deprecated",
                  label: t("lifecycle.deprecated"),
                  tone: "warning",
                },
                {
                  key: "retired",
                  label: t("lifecycle.retired"),
                  tone: "neutral",
                },
              ] satisfies StatusPillOption<LifecycleState>[]
            }
          />
        </div>
        {state === "deprecated" && (
          <div className="grid gap-2 sm:grid-cols-2">
            <SettingsSelect
              label={t("lifecycle.replacedBy")}
              value={replacedBy}
              onChange={(e) => setReplacedBy(e.target.value)}
            >
              <option value="">{t("lifecycle.replacedByEmpty")}</option>
              {replacedByChoices.map((term) => (
                <option key={term.id} value={term.id}>
                  {localize(term.term, localeChain)} ({term.id})
                </option>
              ))}
            </SettingsSelect>
            <label className="flex flex-col gap-1 text-2xs text-foreground">
              <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("lifecycle.deprecatedAt")}
              </span>
              <FormInput
                type="datetime-local"
                value={deprecatedAt}
                onChange={(e) => setDeprecatedAt(e.target.value)}
              />
            </label>
          </div>
        )}
        {state === "retired" && (
          <label className="flex flex-col gap-1 text-2xs text-foreground">
            <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("lifecycle.retiredAt")}
            </span>
            <FormInput
              type="datetime-local"
              value={retiredAt}
              onChange={(e) => setRetiredAt(e.target.value)}
            />
          </label>
        )}
      </fieldset>

      <fieldset className="grid gap-2 rounded border border-divider p-3 sm:grid-cols-2">
        <legend className="px-1 text-2xs font-medium text-foreground">
          {t("validity.legend")}
        </legend>
        <label className="flex flex-col gap-1 text-2xs text-foreground">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("validity.validFrom")}
          </span>
          <FormInput
            type="datetime-local"
            value={validFrom}
            onChange={(e) => setValidFrom(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-2xs text-foreground">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("validity.validTo")}
          </span>
          <FormInput
            type="datetime-local"
            value={validTo}
            onChange={(e) => setValidTo(e.target.value)}
          />
        </label>
      </fieldset>

      <details
        open={governanceOpen}
        onToggle={(e) =>
          setGovernanceOpen((e.target as HTMLDetailsElement).open)
        }
        className="rounded border border-divider"
      >
        <summary className="cursor-pointer px-3 py-2 text-2xs font-medium text-foreground">
          {t("governance.legend")}
        </summary>
        <div className="flex flex-col gap-2 px-3 pb-3 pt-1">
          <SettingsSelect
            label={t("governance.originLabel")}
            value={origin}
            onChange={(e) => setOriginState(e.target.value as OriginKind)}
          >
            <option value="manual">{t("governance.originManual")}</option>
            <option value="derived_from_column">
              {t("governance.originDerivedFromColumn")}
            </option>
            <option value="imported_from">
              {t("governance.originImportedFrom")}
            </option>
          </SettingsSelect>
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
          <div className="flex flex-col gap-1">
            <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("governance.scopeNotes")}
            </span>
            <ChipInput
              values={scopeNotes}
              onChange={setScopeNotes}
              placeholder={t("governance.scopeNotesPlaceholder")}
            />
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("governance.editorialNotes")}
            </span>
            <ChipInput
              values={editorialNotes}
              onChange={setEditorialNotes}
              placeholder={t("governance.editorialNotesPlaceholder")}
            />
          </div>
        </div>
      </details>

      <RelationsField
        selfId={initial?.id}
        initial={relations}
        onChange={setRelations}
        availableTerms={availableTerms ?? []}
      />

      <SaveBar
        dirty={dirty}
        pending={pending}
        onSave={() => {
          void submit({ termDefault } satisfies GlossaryFormSchemaInput);
        }}
        onDiscard={onCancel}
      />
    </form>
  );
}

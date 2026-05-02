"use client";

// ---------------------------------------------------------------------------
// RelationsField — SKOS-style relations editor for a `GlossaryTermDef`.
//
// Surfaces every kind from `TermRelationKind` (broader, narrower,
// related, see_also, exact_match, close_match) so the form expresses
// the same semantic vocabulary the rest of the platform reasons over.
// Each row carries a stable client-side `rowId` (UUID minted on
// creation) used as the React key — keying by index would mis-
// reconcile state on add/remove and silently corrupt the user's
// edits to a row whose neighbour just changed.
//
// Inverse relations (broader ↔ narrower) are NOT mirrored client-
// side: SKOS treats inverse pairs as inference, and storing both
// halves invites inconsistency when one half is removed. The
// inverse-relation hint surfaces in the form so the user knows the
// platform handles the symmetric reasoning at query time.
// ---------------------------------------------------------------------------

import { useState } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import type {
  GlossaryTermDef,
  TermRelation,
  TermRelationKind,
} from "@/lib/api/edit-ops";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

const SKOS_RELATION_KINDS: TermRelationKind[] = [
  "broader",
  "narrower",
  "related",
  "see_also",
  "exact_match",
  "close_match",
];

/** Internal row shape — wire shape is `TermRelation` (no `rowId`).
 *  The id stays in component-local state and is dropped on submit. */
interface InternalRow {
  rowId: string;
  kind: TermRelationKind;
  target: string;
}

interface Props {
  /** Id of the term being edited — excluded from the picker so a
   *  relation cannot loop back on itself. Undefined for create flows. */
  selfId?: string;
  /** Current relations from the parent form's state. Treated as
   *  the source of truth on mount; subsequent edits live in this
   *  component's `rows` and propagate back via `onChange`. */
  initial: TermRelation[];
  /** Called with the wire-shape array (no `rowId`s) on every edit. */
  onChange: (next: TermRelation[]) => void;
  /** Other glossary terms in the ontology — the picker hides
   *  `selfId` and any target already selected by another row. */
  availableTerms: GlossaryTermDef[];
}

function freshRow(seed?: TermRelation): InternalRow {
  return {
    rowId: crypto.randomUUID(),
    kind: seed?.kind ?? "related",
    target: seed?.target ?? "",
  };
}

export function RelationsField({
  selfId,
  initial,
  onChange,
  availableTerms,
}: Props) {
  const t = useTranslations(
    "settings.vocabulary.glossary.form.relations",
  );
  const localeChain = useLocaleChain();
  const [rows, setRows] = useState<InternalRow[]>(() =>
    initial.map(freshRow),
  );

  const propagate = (next: InternalRow[]) => {
    setRows(next);
    onChange(
      next
        .filter((r) => r.target.trim().length > 0)
        .map(({ kind, target }) => ({ kind, target })),
    );
  };

  const update = (rowId: string, patch: Partial<InternalRow>) => {
    propagate(rows.map((r) => (r.rowId === rowId ? { ...r, ...patch } : r)));
  };
  const remove = (rowId: string) => {
    propagate(rows.filter((r) => r.rowId !== rowId));
  };
  const add = () => {
    propagate([...rows, freshRow()]);
  };

  // For a given row's current target, the picker offers `availableTerms`
  // minus self minus targets chosen by sibling rows.
  const pickableTerms = (currentTarget: string) => {
    const taken = new Set(
      rows
        .map((r) => r.target)
        .filter((id) => id && id !== currentTarget),
    );
    return availableTerms.filter(
      (term) => term.id !== selfId && !taken.has(term.id),
    );
  };

  // Add is meaningful only when there is at least one available
  // target the user could pick. Hide the button entirely when there
  // is nothing to add — opening an empty dropdown teaches nothing.
  const someAvailable = availableTerms.some(
    (term) => term.id !== selfId && !rows.some((r) => r.target === term.id),
  );

  return (
    <fieldset className="flex flex-col gap-2 rounded border border-divider p-3">
      <legend className="px-1 text-[11px] font-medium text-foreground">
        {t("title")}
      </legend>
      <p className="text-2xs text-muted-foreground">{t("hint")}</p>
      <p className="text-2xs text-muted-foreground italic">
        {t("inverseHint")}
      </p>

      {rows.length === 0 && (
        <p className="py-1 text-[11px] italic text-muted-foreground">
          {t("empty")}
        </p>
      )}

      {rows.map((row) => {
        const choices = pickableTerms(row.target);
        return (
          <div key={row.rowId} className="flex items-center gap-2">
            <select
              aria-label={t("kindAria")}
              value={row.kind}
              onChange={(e) =>
                update(row.rowId, { kind: e.target.value as TermRelationKind })
              }
              className="rounded border border-divider bg-surface-base px-2 py-1 text-xs"
            >
              {SKOS_RELATION_KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`kind.${k}`)}
                </option>
              ))}
            </select>
            <select
              aria-label={t("targetAria")}
              value={row.target}
              onChange={(e) => update(row.rowId, { target: e.target.value })}
              className="flex-1 rounded border border-divider bg-surface-base px-2 py-1 text-xs"
            >
              <option value="">{t("targetPlaceholder")}</option>
              {choices.map((term) => (
                <option key={term.id} value={term.id}>
                  {localize(term.term, localeChain)} ({term.id})
                </option>
              ))}
              {/* Preserve the current target even when it's not in
                   the picker (e.g. cross-ontology id, or the term
                   was removed by another tab). */}
              {row.target &&
                !choices.some((c) => c.id === row.target) && (
                  <option value={row.target}>{row.target}</option>
                )}
            </select>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={() => remove(row.rowId)}
              aria-label={t("removeAria")}
            >
              {t("remove")}
            </Button>
          </div>
        );
      })}

      {someAvailable && (
        <div>
          <Button type="button" variant="ghost" size="xs" onClick={add}>
            {t("add")}
          </Button>
        </div>
      )}
    </fieldset>
  );
}

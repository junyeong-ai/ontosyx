"use client";

import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";

import {
  useApplyBindingEdits,
  useSuggestBindings,
} from "@/hooks/api/use-binding-suggestions";
import type {
  BindingEditOp,
  PropertyCandidate,
  SuggestBindingsRequest,
} from "@/lib/api/binding-suggestions";
import type { LocalizedText } from "@/types/ontology";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import { EmptyState } from "@/components/ui/empty-state";
import { Checkbox } from "@/components/ui/checkbox";

interface BindingTermContext {
  term_id: string;
  concept_id?: string | null;
  /** Canonical term name with every locale the saved term carries.
   *  The scorer walks every locale, not just the display chain — a
   *  Korean-canonical term with an English alias still matches
   *  English property names. */
  term: LocalizedText;
  aliases?: readonly LocalizedText[];
  description?: LocalizedText;
}

interface ConceptBindingPanelProps {
  ontologyId: string;
  /** Current committed ontology version — required for optimistic
   * locking on the `/edits` batch. */
  expectedVersion: number;
  /** Selected concept lexicalization — drives the suggest call. */
  term: BindingTermContext;
}

export function ConceptBindingPanel({
  ontologyId,
  expectedVersion,
  term,
}: ConceptBindingPanelProps) {
  const t = useTranslations("settings.conceptBinding");

  // Selected candidates for the batch bind action. Keyed by
  // `kind:type_id:property_id` so the same property can't be picked
  // twice.
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const suggest = useSuggestBindings(ontologyId);
  const apply = useApplyBindingEdits(ontologyId, {
    onSuccess: (receipt) => {
      toast.success(
        t("toast.applied", { n: receipt.applied_operations }),
      );
      setSelected(new Set());
    },
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : t("toast.failed")),
  });

  const candidates = suggest.data?.candidates ?? [];

  const localeChain = useLocaleChain("admin");
  const displayTerm = localize(term.term, localeChain);

  // Auto-score on term change. The mutation hook's `mutate` is
  // stable across renders (react-query guarantees) so the effect
  // fires only when the term identity / fields shift. Selection
  // clears alongside — candidates from the prior term are no
  // longer referenceable.
  //
  // The dependency key is the term_id alone — the LocalizedText
  // shape carries every locale, so any meaningful change is
  // observable through the saved-term identity bump that drives
  // the parent's re-mount.
  useEffect(() => {
    setSelected(new Set());
    if (!term.term_id || !displayTerm) return;
    const body: SuggestBindingsRequest = {
      term: term.term,
      aliases: [...(term.aliases ?? [])],
      description: term.description,
      term_id: term.term_id,
    };
    suggest.mutate(body);
    // suggest.mutate is stable; the term_id bump drives the
    // refetch. Adding the mutate fn would re-fire spuriously.
  }, [term.term_id, suggest.mutate, term.term, term.description, term.aliases, displayTerm]);

  const candidateKey = (c: PropertyCandidate) =>
    `${c.owner_kind}:${c.owner_type_id}:${c.property_id}`;

  const toggle = (key: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const onBatchBind = () => {
    const conceptId = term.concept_id;
    if (selected.size === 0 || !conceptId) return;
    const ops: BindingEditOp[] = candidates
      .filter((c) => selected.has(candidateKey(c)))
      .map(
        (c): BindingEditOp => ({
          op: "bind_property",
          owner:
            c.owner_kind === "node"
              ? { kind: "node", type_id: c.owner_type_id }
              : { kind: "edge", type_id: c.owner_type_id },
          property_id: c.property_id,
          binding: { kind: "concept", id: conceptId },
        }),
      );
    apply.mutate({
      expected_version: expectedVersion,
      operations: ops,
      message: `batch bind ${ops.length} properties → concept ${conceptId}`,
    });
  };

  const anySelected = selected.size > 0;
  const canBind = Boolean(term.concept_id);
  const selectedCount = useMemo(() => selected.size, [selected]);

  return (
    <div className="flex h-full flex-col gap-3 overflow-hidden text-xs">
      <header className="flex items-center justify-between gap-2 border-b border-divider pb-2">
        <div className="min-w-0">
          <p className="text-2xs uppercase tracking-wider text-foreground-muted">
            {t("embedded.targetLabel")}
          </p>
          <p className="truncate font-medium text-foreground-strong">
            {displayTerm}
          </p>
          <p className="truncate font-mono text-2xs text-foreground-muted">
            {term.term_id}
          </p>
        </div>
        <button
          type="button"
          onClick={() =>
            suggest.mutate({
              term: term.term,
              aliases: [...(term.aliases ?? [])],
              description: term.description,
              term_id: term.term_id,
            })
          }
          disabled={suggest.isPending}
          className="shrink-0 rounded border border-concept-border px-2 py-1 text-2xs font-medium text-concept-foreground hover:bg-concept-surface disabled:opacity-50"
        >
          {suggest.isPending
            ? t("actions.searching")
            : t("embedded.rescore")}
        </button>
      </header>

      <div className="flex-1 overflow-y-auto">
        {suggest.isPending && candidates.length === 0 && (
          <EmptyState variant="compact" title={t("actions.searching")} />
        )}

        {suggest.isSuccess && candidates.length === 0 && (
          <EmptyState variant="compact" title={t("empty")} />
        )}

        {candidates.length > 0 && (
          <table className="w-full border-collapse text-xs">
            <thead>
              <tr className="border-b border-divider text-start text-2xs uppercase tracking-wider text-foreground-muted">
                <th className="w-8 py-2 pe-2"></th>
                <th className="py-2 pe-3 font-medium">
                  {t("columns.owner")}
                </th>
                <th className="py-2 pe-3 font-medium">
                  {t("columns.property")}
                </th>
                <th className="py-2 pe-2 font-medium">
                  {t("columns.score")}
                </th>
              </tr>
            </thead>
            <tbody>
              {candidates.map((c) => {
                const key = candidateKey(c);
                const picked = selected.has(key);
                return (
                  <tr
                    key={key}
                    className="border-b border-divider-soft hover:bg-surface-raised"
                  >
                    <td className="py-1.5 pe-2">
                      <Checkbox
                        checked={picked}
                        onChange={() => toggle(key)}
                        aria-label={t("rowAria", {
                          owner: c.owner_label || c.owner_type_id,
                          property: c.property_name,
                        })}
                      />
                    </td>
                    <td className="py-1.5 pe-3">
                      <span className="rounded bg-surface-inset px-1 py-0.5 text-2xs uppercase text-foreground-muted">
                        {c.owner_kind}
                      </span>{" "}
                      <span className="text-2xs font-medium">
                        {c.owner_label || c.owner_type_id}
                      </span>
                    </td>
                    <td className="py-1.5 pe-3 font-mono text-2xs">
                      {c.property_name}
                    </td>
                    <td className="py-1.5 pe-2">
                      <ScoreBar score={c.score} />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {anySelected && (
        <footer className="flex items-center justify-end gap-2 border-t border-divider pt-2">
          <button
            type="button"
            onClick={onBatchBind}
            disabled={!canBind || apply.isPending}
            className="rounded bg-brand-solid px-3 py-1 text-2xs font-medium text-foreground-onbrand hover:bg-brand-solid disabled:opacity-50"
          >
            {apply.isPending
              ? t("actions.binding")
              : t("actions.bind", { n: selectedCount })}
          </button>
        </footer>
      )}
    </div>
  );
}

function ScoreBar({ score }: { score: number }) {
  const pct = Math.round(Math.max(0, Math.min(1, score)) * 100);
  return (
    <div className="flex items-center gap-1.5">
      <div className="h-1 w-10 overflow-hidden rounded bg-surface-inset">
        <div
          className="h-full bg-concept-foreground"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="tabular-nums text-2xs text-foreground-muted">
        {pct}%
      </span>
    </div>
  );
}

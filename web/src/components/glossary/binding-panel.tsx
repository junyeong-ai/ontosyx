"use client";

// Glossary batch binding panel.
//
// Given an ontology + a selected glossary term, scores every
// PropertyDef in the graph against the term, surfaces the top N
// candidates in a checkable table, and batch-commits
// `bind_property` ops through `/edits`. Embedded into the right
// pane of the glossary workbench under the "Bindings" tab; the
// term context comes from the workbench's selected term, so this
// panel never asks the user to retype it.

import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import {
  useApplyBindingEdits,
  useSuggestBindings,
} from "@/hooks/api/use-binding-suggestions";
import type {
  BindingEditOp,
  PropertyCandidate,
  SuggestBindingsRequest,
} from "@/lib/api/binding-suggestions";

/**
 * Selected-term context passed in from the workbench. Mirrors the
 * fields the suggest scorer consumes — id + label + (optional)
 * aliases / description. The panel never mutates this; refetching
 * happens via the local "Re-score" button when the operator wants
 * fresh signals after editing the term.
 */
export interface BindingTermContext {
  term_id: string;
  term: string;
  aliases?: string[];
  description?: string;
}

export interface GlossaryBindingPanelProps {
  ontologyId: string;
  /** Current committed ontology version — required for optimistic
   * locking on the `/edits` batch. */
  expectedVersion: number;
  /** Selected glossary term — drives the suggest call. */
  term: BindingTermContext;
}

export function GlossaryBindingPanel({
  ontologyId,
  expectedVersion,
  term,
}: GlossaryBindingPanelProps) {
  const t = useTranslations("settings.glossaryBinding");

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

  // Auto-score on term change. The mutation hook's `mutate` is
  // stable across renders (react-query guarantees) so the effect
  // fires only when the term identity / fields shift. Selection
  // clears alongside — candidates from the prior term are no
  // longer referenceable.
  const aliasesKey = (term.aliases ?? []).join("|");
  useEffect(() => {
    setSelected(new Set());
    if (!term.term_id || !term.term) return;
    const body: SuggestBindingsRequest = {
      term: term.term,
      aliases: term.aliases ?? [],
      description: term.description?.trim()
        ? term.description.trim()
        : undefined,
      term_id: term.term_id,
    };
    suggest.mutate(body);
    // suggest.mutate is stable; depending on `term` fields drives
    // the refetch. Adding the mutate fn would re-fire spuriously.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [term.term_id, term.term, term.description, aliasesKey]);

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
    if (selected.size === 0) return;
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
          binding: {
            kind: "glossary",
            id: term.term_id,
          },
        }),
      );
    apply.mutate({
      expected_version: expectedVersion,
      operations: ops,
      message: `batch bind ${ops.length} properties → glossary ${term.term_id}`,
    });
  };

  const anySelected = selected.size > 0;
  const selectedCount = useMemo(() => selected.size, [selected]);

  return (
    <div className="flex h-full flex-col gap-3 overflow-hidden text-xs">
      <header className="flex items-center justify-between gap-2 border-b border-zinc-200 pb-2 dark:border-zinc-800">
        <div className="min-w-0">
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
            {t("embedded.targetLabel")}
          </p>
          <p className="truncate font-medium text-zinc-900 dark:text-zinc-100">
            {term.term}
          </p>
          <p className="truncate font-mono text-[10px] text-muted-foreground">
            {term.term_id}
          </p>
        </div>
        <button
          type="button"
          onClick={() =>
            suggest.mutate({
              term: term.term,
              aliases: term.aliases ?? [],
              description: term.description?.trim()
                ? term.description.trim()
                : undefined,
              term_id: term.term_id,
            })
          }
          disabled={suggest.isPending}
          className="shrink-0 rounded border border-violet-300 px-2 py-1 text-[10px] font-medium text-violet-700 hover:bg-violet-50 disabled:opacity-50 dark:border-violet-800 dark:text-violet-300 dark:hover:bg-violet-950/40"
        >
          {suggest.isPending
            ? t("actions.searching")
            : t("embedded.rescore")}
        </button>
      </header>

      <div className="flex-1 overflow-y-auto">
        {suggest.isPending && candidates.length === 0 && (
          <p className="py-6 text-center text-[11px] text-muted-foreground">
            {t("actions.searching")}
          </p>
        )}

        {suggest.isSuccess && candidates.length === 0 && (
          <p className="py-6 text-center text-[11px] text-muted-foreground">
            {t("empty")}
          </p>
        )}

        {candidates.length > 0 && (
          <table className="w-full border-collapse text-xs">
            <thead>
              <tr className="border-b border-zinc-200 text-left text-[10px] uppercase tracking-wider text-muted-foreground dark:border-zinc-800">
                <th className="w-8 py-2 pr-2"></th>
                <th className="py-2 pr-3 font-medium">
                  {t("columns.owner")}
                </th>
                <th className="py-2 pr-3 font-medium">
                  {t("columns.property")}
                </th>
                <th className="py-2 pr-2 font-medium">
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
                    className="border-b border-zinc-100 hover:bg-zinc-50 dark:border-zinc-800/50 dark:hover:bg-zinc-800/30"
                  >
                    <td className="py-1.5 pr-2">
                      <input
                        type="checkbox"
                        checked={picked}
                        onChange={() => toggle(key)}
                        aria-label={t("rowAria", {
                          owner: c.owner_label || c.owner_type_id,
                          property: c.property_name,
                        })}
                      />
                    </td>
                    <td className="py-1.5 pr-3">
                      <span className="rounded bg-zinc-100 px-1 py-0.5 text-[9px] uppercase text-muted-foreground dark:bg-zinc-800">
                        {c.owner_kind}
                      </span>{" "}
                      <span className="text-[11px] font-medium">
                        {c.owner_label || c.owner_type_id}
                      </span>
                    </td>
                    <td className="py-1.5 pr-3 font-mono text-[10px]">
                      {c.property_name}
                    </td>
                    <td className="py-1.5 pr-2">
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
        <footer className="flex items-center justify-end gap-2 border-t border-zinc-200 pt-2 dark:border-zinc-800">
          <button
            type="button"
            onClick={onBatchBind}
            disabled={apply.isPending}
            className="rounded bg-emerald-600 px-3 py-1 text-[11px] font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
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
      <div className="h-1 w-10 overflow-hidden rounded bg-zinc-200 dark:bg-zinc-800">
        <div
          className="h-full bg-violet-500"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="tabular-nums text-[9px] text-muted-foreground">
        {pct}%
      </span>
    </div>
  );
}

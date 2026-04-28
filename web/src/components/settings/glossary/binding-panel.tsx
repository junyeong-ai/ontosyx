"use client";

// Phase 4.5 reverse direction — Glossary batch binding panel.
//
// Given an ontology + a term (either existing or a draft), scores
// every PropertyDef in the graph, surfaces the top N candidates in
// a checkable table, and batch-commits `bind_property` ops
// through `/edits`. Used standalone on the /settings/glossary
// bindings page, but structured as a self-contained component so a
// future Glossary CRUD editor can embed it in a side panel.

import { useMemo, useState } from "react";
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

export interface GlossaryBindingPanelProps {
  ontologyId: string;
  /** Current committed ontology version — required for optimistic
   * locking on the `/edits` batch. */
  expectedVersion: number;
}

export function GlossaryBindingPanel({
  ontologyId,
  expectedVersion,
}: GlossaryBindingPanelProps) {
  const t = useTranslations("settings.glossaryBinding");

  // Term draft — the inputs the scorer uses.
  const [term, setTerm] = useState("");
  const [aliases, setAliases] = useState("");
  const [description, setDescription] = useState("");
  const [termId, setTermId] = useState("");

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

  const onSearch = () => {
    if (!term.trim()) {
      toast.error(t("errors.termRequired"));
      return;
    }
    const body: SuggestBindingsRequest = {
      term: term.trim(),
      aliases: aliases
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      description: description.trim() ? description.trim() : undefined,
      term_id: termId.trim() ? termId.trim() : undefined,
    };
    setSelected(new Set());
    suggest.mutate(body);
  };

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
    if (!termId.trim()) {
      toast.error(t("errors.termIdRequired"));
      return;
    }
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
            id: termId.trim(),
          },
        }),
      );
    apply.mutate({
      expected_version: expectedVersion,
      operations: ops,
      message: `batch bind ${ops.length} properties → glossary ${termId.trim()}`,
    });
  };

  const anySelected = selected.size > 0;

  const selectedCount = useMemo(() => selected.size, [selected]);

  return (
    <div className="flex flex-col gap-4">
      <section className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <Field
          id="binding-term"
          label={t("fields.term.label")}
          required
        >
          <input
            id="binding-term"
            value={term}
            onChange={(e) => setTerm(e.target.value)}
            placeholder={t("fields.term.placeholder")}
            className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 text-sm dark:border-zinc-600 dark:bg-zinc-900"
          />
        </Field>
        <Field id="binding-term-id" label={t("fields.termId.label")}>
          <input
            id="binding-term-id"
            value={termId}
            onChange={(e) => setTermId(e.target.value)}
            placeholder={t("fields.termId.placeholder")}
            className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 font-mono text-xs dark:border-zinc-600 dark:bg-zinc-900"
          />
          <p className="mt-1 text-[10px] text-muted-foreground">
            {t("fields.termId.hint")}
          </p>
        </Field>
        <Field id="binding-aliases" label={t("fields.aliases.label")}>
          <input
            id="binding-aliases"
            value={aliases}
            onChange={(e) => setAliases(e.target.value)}
            placeholder={t("fields.aliases.placeholder")}
            className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 text-sm dark:border-zinc-600 dark:bg-zinc-900"
          />
        </Field>
        <Field
          id="binding-description"
          label={t("fields.description.label")}
        >
          <textarea
            id="binding-description"
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("fields.description.placeholder")}
            className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 text-sm dark:border-zinc-600 dark:bg-zinc-900"
          />
        </Field>
      </section>

      <div className="flex items-center justify-between">
        <button
          type="button"
          onClick={onSearch}
          disabled={suggest.isPending}
          className="rounded bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-700 disabled:opacity-50"
        >
          {suggest.isPending ? t("actions.searching") : t("actions.search")}
        </button>
        {anySelected && (
          <button
            type="button"
            onClick={onBatchBind}
            disabled={apply.isPending}
            className="rounded bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
          >
            {apply.isPending
              ? t("actions.binding")
              : t("actions.bind", { n: selectedCount })}
          </button>
        )}
      </div>

      {suggest.isSuccess && candidates.length === 0 && (
        <p className="py-4 text-center text-xs text-muted-foreground">
          {t("empty")}
        </p>
      )}

      {candidates.length > 0 && (
        <table className="w-full border-collapse text-xs">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-[10px] uppercase tracking-wider text-muted-foreground dark:border-zinc-800">
              <th className="w-8 py-2 pr-2"></th>
              <th className="py-2 pr-4 font-medium">
                {t("columns.owner")}
              </th>
              <th className="py-2 pr-4 font-medium">
                {t("columns.property")}
              </th>
              <th className="py-2 pr-4 font-medium">
                {t("columns.score")}
              </th>
              <th className="py-2 pr-4 font-medium">
                {t("columns.signals")}
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
                  <td className="py-2 pr-2">
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
                  <td className="py-2 pr-4">
                    <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground dark:bg-zinc-800">
                      {c.owner_kind}
                    </span>{" "}
                    <span className="font-medium">
                      {c.owner_label || c.owner_type_id}
                    </span>
                  </td>
                  <td className="py-2 pr-4 font-mono">{c.property_name}</td>
                  <td className="py-2 pr-4">
                    <ScoreBar score={c.score} />
                  </td>
                  <td className="py-2 pr-4 text-[10px] text-muted-foreground">
                    {c.signals.map((s) => s.kind).join(", ") || "—"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}

function Field({
  id,
  label,
  required,
  children,
}: {
  id: string;
  label: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label
        htmlFor={id}
        className="mb-1 block text-[11px] font-medium text-zinc-700 dark:text-zinc-300"
      >
        {label}
        {required && <span className="ml-1 text-rose-500">*</span>}
      </label>
      {children}
    </div>
  );
}

function ScoreBar({ score }: { score: number }) {
  const pct = Math.round(Math.max(0, Math.min(1, score)) * 100);
  return (
    <div className="flex items-center gap-2">
      <div className="h-1.5 w-16 overflow-hidden rounded bg-zinc-200 dark:bg-zinc-800">
        <div
          className="h-full bg-violet-500"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="tabular-nums text-[10px] text-muted-foreground">
        {pct}%
      </span>
    </div>
  );
}

"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import {
  useDecideStaleProposal,
  useStaleProposals,
} from "@/hooks/api/use-quality";
import {
  applyOntologyEdits,
  type BindingEditOp,
} from "@/lib/api/binding-suggestions";
import {
  listTypeCandidates,
  type TypeCandidate,
} from "@/lib/api/quality";
import type { StaleConceptProposal } from "@/types/api";

type TabKey = "pending" | "decided";

export default function StaleConceptsPage() {
  const t = useTranslations("settings.staleConcepts");
  const [tab, setTab] = useState<TabKey>("pending");
  const [editing, setEditing] = useState<StaleConceptProposal | null>(null);
  const [picker, setPicker] = useState<{
    proposal: StaleConceptProposal;
    candidates: TypeCandidate[];
  } | null>(null);

  const pending = useStaleProposals(false);
  const all = useStaleProposals(true);

  const decided = useMemo(
    () => (all.data ?? []).filter((r) => r.decision !== "pending"),
    [all.data],
  );

  const emitDeprecate = async (
    proposal: StaleConceptProposal,
    candidate: TypeCandidate,
    reason?: string,
  ) => {
    const expected = Number.parseInt(candidate.current_version, 10);
    if (!Number.isFinite(expected)) {
      toast.error(t("toast.emitFailed"));
      return;
    }
    const kind = normaliseKind(proposal.type_kind);
    const op: BindingEditOp =
      kind === "edge"
        ? {
            op: "deprecate_edge_type",
            id: proposal.type_id,
          }
        : {
            op: "deprecate_node_type",
            id: proposal.type_id,
          };
    try {
      await applyOntologyEdits(candidate.ontology_id, {
        expected_version: expected,
        operations: [op],
        message: reason
          ? `deprecated via stale-proposal approval: ${reason}`
          : "deprecated via stale-proposal approval",
      });
      toast.success(
        t("toast.deprecated", { ontology: candidate.ontology_name }),
      );
    } catch (e) {
      toast.error(
        e instanceof Error ? e.message : t("toast.emitFailed"),
      );
    }
  };

  const decide = useDecideStaleProposal({
    onSuccess: async (row, vars) => {
      toast.success(
        row.decision === "approved"
          ? t("toast.approved")
          : t("toast.dismissed"),
      );
      setEditing(null);
      if (row.decision !== "approved") return;

      const kind = normaliseKind(row.type_kind);
      let candidates: TypeCandidate[] = [];
      try {
        candidates = await listTypeCandidates(row.type_id, kind);
      } catch {
        // Lookup failure shouldn't roll back the approval — the
        // row is already recorded. Warn and give up on the
        // follow-up edit.
        toast.error(t("toast.lookupFailed"));
        return;
      }
      const usable = candidates.filter((c) => !c.deprecated_at);
      if (usable.length === 0) {
        if (candidates.length > 0) {
          toast.info(t("toast.alreadyDeprecated"));
        } else {
          toast.info(t("toast.noCandidate"));
        }
        return;
      }
      if (usable.length === 1) {
        await emitDeprecate(row, usable[0], vars.reason);
        return;
      }
      setPicker({ proposal: row, candidates: usable });
    },
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : t("toast.failed")),
  });

  const active = tab === "pending" ? (pending.data ?? []) : decided;
  const loading = tab === "pending" ? pending.isLoading : all.isLoading;

  return (
    <div className="flex flex-col gap-4">
      <header>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("title")}
        </h1>
        <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
          {t("subtitle")}
        </p>
      </header>

      <nav
        aria-label={t("tabsLabel")}
        className="flex gap-1 border-b border-zinc-200 dark:border-zinc-800"
      >
        {(["pending", "decided"] as const).map((k) => (
          <button
            key={k}
            type="button"
            aria-pressed={tab === k}
            onClick={() => setTab(k)}
            className={`relative px-3 py-2 text-xs font-medium ${
              tab === k
                ? "text-violet-700 dark:text-violet-400"
                : "text-muted-foreground hover:text-zinc-700 dark:hover:text-zinc-300"
            }`}
          >
            {t(`tabs.${k}`)}
            <span className="ml-1 rounded bg-zinc-100 px-1 text-[10px] dark:bg-zinc-800">
              {k === "pending" ? (pending.data?.length ?? 0) : decided.length}
            </span>
            {tab === k && (
              <span className="absolute inset-x-0 -bottom-px h-0.5 bg-violet-500" />
            )}
          </button>
        ))}
      </nav>

      {loading && (
        <p className="py-6 text-center text-xs text-muted-foreground">
          {t("loading")}
        </p>
      )}

      {!loading && active.length === 0 && (
        <p className="py-6 text-center text-xs text-muted-foreground">
          {t(`empty.${tab}`)}
        </p>
      )}

      {!loading && active.length > 0 && (
        <table className="w-full border-collapse text-xs">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-[10px] uppercase tracking-wider text-muted-foreground dark:border-zinc-800">
              <th className="py-2 pr-4 font-medium">{t("columns.type")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.kind")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.daysSince")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.proposedAt")}</th>
              {tab === "decided" && (
                <>
                  <th className="py-2 pr-4 font-medium">{t("columns.decision")}</th>
                  <th className="py-2 pr-4 font-medium">{t("columns.reason")}</th>
                </>
              )}
              {tab === "pending" && (
                <th className="py-2 pr-4 text-right font-medium">
                  {t("columns.actions")}
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {active.map((row) => (
              <tr
                key={row.id}
                className="border-b border-zinc-100 dark:border-zinc-800/50"
              >
                <td className="py-2 pr-4 font-mono">{row.type_id}</td>
                <td className="py-2 pr-4 text-muted-foreground">
                  {row.type_kind}
                </td>
                <td className="py-2 pr-4">{row.days_since_last_use}</td>
                <td className="py-2 pr-4 text-muted-foreground">
                  {new Date(row.proposed_at).toLocaleDateString()}
                </td>
                {tab === "decided" && (
                  <>
                    <td className="py-2 pr-4">
                      <DecisionBadge decision={row.decision} />
                    </td>
                    <td className="py-2 pr-4 text-muted-foreground">
                      {row.reason ?? "—"}
                    </td>
                  </>
                )}
                {tab === "pending" && (
                  <td className="py-2 pr-4 text-right">
                    <button
                      type="button"
                      onClick={() => setEditing(row)}
                      className="rounded px-2 py-1 text-[10px] font-medium text-violet-700 hover:bg-violet-50 dark:text-violet-400 dark:hover:bg-violet-950/40"
                    >
                      {t("actions.review")}
                    </button>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {editing && (
        <DecisionModal
          proposal={editing}
          busy={decide.isPending}
          onCancel={() => setEditing(null)}
          onSubmit={(decision, reason) =>
            decide.mutate({ id: editing.id, decision, reason })
          }
        />
      )}

      {picker && (
        <CandidatePickerModal
          proposal={picker.proposal}
          candidates={picker.candidates}
          onCancel={() => setPicker(null)}
          onPick={async (candidate) => {
            const snapshot = picker;
            setPicker(null);
            await emitDeprecate(snapshot.proposal, candidate);
          }}
        />
      )}
    </div>
  );
}

function DecisionBadge({ decision }: { decision: StaleConceptProposal["decision"] }) {
  const t = useTranslations("settings.staleConcepts.decisionLabel");
  if (decision === "approved") {
    return (
      <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300">
        {t("approved")}
      </span>
    );
  }
  if (decision === "dismissed") {
    return (
      <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-muted-foreground dark:bg-zinc-800">
        {t("dismissed")}
      </span>
    );
  }
  return (
    <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
      {t("pending")}
    </span>
  );
}

function DecisionModal({
  proposal,
  busy,
  onSubmit,
  onCancel,
}: {
  proposal: StaleConceptProposal;
  busy: boolean;
  onSubmit: (decision: "approved" | "dismissed", reason?: string) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.staleConcepts.modal");
  const [reason, setReason] = useState("");

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="stale-modal-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-900/40 p-4 backdrop-blur-sm"
    >
      <div className="w-full max-w-md rounded-lg border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-950">
        <header className="border-b border-zinc-200 px-5 py-3 dark:border-zinc-800">
          <h2
            id="stale-modal-title"
            className="text-sm font-semibold text-zinc-900 dark:text-zinc-100"
          >
            {t("title")}
          </h2>
          <p className="mt-1 text-[11px] text-muted-foreground">
            <span className="font-mono">{proposal.type_id}</span>
            {" · "}
            {t("daysSince", { days: proposal.days_since_last_use })}
          </p>
        </header>

        <div className="px-5 py-4 text-xs">
          <label
            htmlFor="stale-reason"
            className="mb-1 block text-[11px] font-medium text-zinc-700 dark:text-zinc-300"
          >
            {t("reasonLabel")}
          </label>
          <textarea
            id="stale-reason"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={3}
            placeholder={t("reasonPlaceholder")}
            className="w-full rounded border border-zinc-300 bg-white px-2 py-1.5 text-xs dark:border-zinc-600 dark:bg-zinc-900"
          />
        </div>

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
            onClick={() => onSubmit("dismissed", reason.trim() || undefined)}
            disabled={busy}
            className="rounded bg-zinc-100 px-3 py-1.5 text-xs font-medium text-zinc-700 hover:bg-zinc-200 disabled:opacity-50 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700"
          >
            {t("dismiss")}
          </button>
          <button
            type="button"
            onClick={() => onSubmit("approved", reason.trim() || undefined)}
            disabled={busy}
            className="rounded bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
          >
            {busy ? t("saving") : t("approve")}
          </button>
        </footer>
      </div>
    </div>
  );
}

// Shown when multiple ontologies carry the same logical id. The
// admin picks which ontology receives the DeprecateType edit; cancel
// leaves the approval recorded but emits no edit (the audit trail
// still reflects the decision, per the patent's HITL contract).
function CandidatePickerModal({
  proposal,
  candidates,
  onPick,
  onCancel,
}: {
  proposal: StaleConceptProposal;
  candidates: TypeCandidate[];
  onPick: (candidate: TypeCandidate) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.staleConcepts.picker");
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="stale-picker-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-900/40 p-4 backdrop-blur-sm"
    >
      <div className="w-full max-w-lg rounded-lg border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-950">
        <header className="border-b border-zinc-200 px-5 py-3 dark:border-zinc-800">
          <h2
            id="stale-picker-title"
            className="text-sm font-semibold text-zinc-900 dark:text-zinc-100"
          >
            {t("title")}
          </h2>
          <p className="mt-1 text-[11px] text-muted-foreground">
            <span className="font-mono">{proposal.type_id}</span>
            {" · "}
            {t("countHit", { count: candidates.length })}
          </p>
        </header>
        <ul className="max-h-72 overflow-y-auto divide-y divide-zinc-100 dark:divide-zinc-800/60">
          {candidates.map((c) => (
            <li key={`${c.ontology_id}-${c.current_version}`}>
              <button
                type="button"
                onClick={() => onPick(c)}
                className="flex w-full items-start justify-between gap-3 px-5 py-3 text-left text-xs hover:bg-violet-50 dark:hover:bg-violet-950/40"
              >
                <div className="min-w-0">
                  <p className="truncate font-medium text-zinc-900 dark:text-zinc-100">
                    {c.ontology_name}
                  </p>
                  <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                    <span className="font-mono">{c.label}</span>
                    {" · "}v{c.current_version}
                  </p>
                </div>
                <span className="rounded bg-emerald-50 px-2 py-0.5 text-[10px] text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300">
                  {t("pick")}
                </span>
              </button>
            </li>
          ))}
        </ul>
        <footer className="flex items-center justify-end border-t border-zinc-200 px-5 py-3 dark:border-zinc-800">
          <button
            type="button"
            onClick={onCancel}
            className="rounded px-3 py-1.5 text-xs text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            {t("skip")}
          </button>
        </footer>
      </div>
    </div>
  );
}

function normaliseKind(raw: string): "node" | "edge" {
  // Match the kind tokens the backend accepts — lowercase variants
  // and the `NodeType`/`EdgeType` aliases written by the query-signal
  // pipeline. Default to node so an unknown kind still produces a
  // valid request (the backend 404s rather than 400).
  const lc = raw.toLowerCase();
  if (lc === "edge" || lc === "edgetype" || lc === "edge_type") return "edge";
  return "node";
}

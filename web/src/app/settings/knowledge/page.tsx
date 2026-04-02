"use client";

import { useEffect, useState, useCallback } from "react";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import type { KnowledgeEntry, KnowledgeStatus, KnowledgeKind } from "@/types/api";
import {
  listKnowledge,
  deleteKnowledge,
  updateKnowledgeStatus,
  bulkReviewKnowledge,
} from "@/lib/api/knowledge";
import { useAuth } from "@/lib/use-auth";
import { cn } from "@/lib/cn";

const KIND: Record<string, { cls: string; label: string }> = {
  correction: { cls: "text-blue-700 bg-blue-50 ring-blue-600/20 dark:text-blue-400 dark:bg-blue-950/40 dark:ring-blue-400/20", label: "Correction" },
  hint: { cls: "text-violet-700 bg-violet-50 ring-violet-600/20 dark:text-violet-400 dark:bg-violet-950/40 dark:ring-violet-400/20", label: "Hint" },
};

const STATUS: Record<string, { dot: string; label: string }> = {
  approved: { dot: "bg-emerald-500", label: "Approved" },
  draft: { dot: "bg-zinc-400", label: "Draft" },
  stale: { dot: "bg-amber-500", label: "Stale" },
  deprecated: { dot: "bg-zinc-300 dark:bg-zinc-600", label: "Deprecated" },
};

export default function KnowledgePage() {
  const [entries, setEntries] = useState<KnowledgeEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState("");
  const [kindFilter, setKindFilter] = useState("");
  const { isAdmin } = useAuth();

  const load = useCallback(async () => {
    try {
      const page = await listKnowledge({ status: statusFilter || undefined, kind: kindFilter || undefined, limit: 100 });
      setEntries(page.items);
    } catch { toast.error("Failed to load knowledge entries"); }
    finally { setLoading(false); }
  }, [statusFilter, kindFilter]);

  useEffect(() => { load(); }, [load]);

  const handleStatus = useCallback(async (id: string, status: KnowledgeStatus) => {
    try {
      await updateKnowledgeStatus(id, status);
      setEntries((p) => p.map((e) => (e.id === id ? { ...e, status } : e)));
      toast.success(`Status changed to ${status}`);
    } catch { toast.error("Failed to update status"); }
  }, []);

  const handleDelete = async (id: string) => {
    try {
      await deleteKnowledge(id);
      setEntries((p) => p.filter((e) => e.id !== id));
      if (expandedId === id) setExpandedId(null);
      toast.success("Deleted");
    } catch { toast.error("Delete failed"); }
  };

  const staleCount = entries.filter((e) => e.status === "stale").length;

  if (!isAdmin) {
    return <div className="flex h-full items-center justify-center text-sm text-zinc-500">Admin access required.</div>;
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto">
        {/* Header */}
        <div className="flex items-start justify-between">
          <div>
            <h1 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">Knowledge Base</h1>
            <p className="mt-1 text-sm text-zinc-500">
              Learned corrections from query failures and admin-created hints.
            </p>
          </div>
          <div className="flex items-center gap-2">
            {staleCount > 0 && (
              <button
                onClick={async () => {
                  const ids = entries.filter((e) => e.status === "stale").map((e) => e.id);
                  try { await bulkReviewKnowledge(ids, "deprecated"); toast.success(`${ids.length} deprecated`); load(); }
                  catch { toast.error("Failed"); }
                }}
                className="rounded-md bg-amber-500 px-3 py-1.5 text-xs font-semibold text-white hover:bg-amber-600 transition"
              >
                Review {staleCount} stale
              </button>
            )}
          </div>
        </div>

        {/* Filters */}
        <div className="mt-4 flex items-center gap-3">
          <select value={statusFilter} onChange={(e) => setStatusFilter(e.target.value)}
            className="rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-sm text-zinc-700 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
            <option value="">All status</option>
            <option value="approved">Approved</option>
            <option value="draft">Draft</option>
            <option value="stale">Stale</option>
            <option value="deprecated">Deprecated</option>
          </select>
          <select value={kindFilter} onChange={(e) => setKindFilter(e.target.value)}
            className="rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-sm text-zinc-700 focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
            <option value="">All kinds</option>
            <option value="correction">Correction</option>
            <option value="hint">Hint</option>
          </select>
          <span className="ml-auto text-sm tabular-nums text-zinc-400">{entries.length} entries</span>
        </div>

        {/* Content */}
        <div className="mt-5">
          {loading ? (
            <div className="flex justify-center py-16"><Spinner /></div>
          ) : entries.length === 0 ? (
            <div className="rounded-xl border border-dashed border-zinc-300 px-6 py-16 text-center dark:border-zinc-700">
              <p className="text-sm text-zinc-500">No knowledge entries yet.</p>
              <p className="mt-1 text-xs text-zinc-400">
                Entries are auto-created when the agent recovers from query failures, or manually as hints.
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              {entries.map((entry) => (
                <EntryCard
                  key={entry.id}
                  entry={entry}
                  isExpanded={expandedId === entry.id}
                  onToggle={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
                  onApprove={() => handleStatus(entry.id, "approved")}
                  onDeprecate={() => handleStatus(entry.id, "deprecated")}
                  onDelete={() => handleDelete(entry.id)}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function EntryCard({
  entry, isExpanded, onToggle, onApprove, onDeprecate, onDelete,
}: {
  entry: KnowledgeEntry;
  isExpanded: boolean;
  onToggle: () => void;
  onApprove: () => void;
  onDeprecate: () => void;
  onDelete: () => void;
}) {
  const k = KIND[entry.kind] ?? KIND.correction;
  const s = STATUS[entry.status] ?? STATUS.draft;

  return (
    <div className={cn(
      "rounded-xl border transition-all",
      isExpanded
        ? "border-emerald-200 bg-white shadow-sm dark:border-emerald-800/40 dark:bg-zinc-900"
        : "border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700",
    )}>
      {/* Summary row — always visible */}
      <button onClick={onToggle} className="flex w-full items-center gap-3 px-4 py-3 text-left">
        {/* Kind badge */}
        <span className={cn("shrink-0 rounded-md px-2 py-0.5 text-[11px] font-semibold ring-1 ring-inset", k.cls)}>
          {k.label}
        </span>

        {/* Status dot + label */}
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500">
          <span className={cn("h-2 w-2 rounded-full", s.dot)} />
          {s.label}
        </span>

        {/* Title */}
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
          {entry.title}
        </span>

        {/* Labels (compact) */}
        <span className="hidden shrink-0 items-center gap-1 sm:flex">
          {entry.affected_labels.slice(0, 3).map((l) => (
            <span key={l} className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">{l}</span>
          ))}
          {entry.affected_labels.length > 3 && (
            <span className="text-[10px] text-zinc-400">+{entry.affected_labels.length - 3}</span>
          )}
        </span>

        {/* Confidence */}
        <span className="shrink-0 text-xs tabular-nums text-zinc-400">{(entry.confidence * 100).toFixed(0)}%</span>

        {/* Chevron */}
        <svg className={cn("h-4 w-4 shrink-0 text-zinc-400 transition-transform", isExpanded && "rotate-180")}
          fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Expanded detail */}
      {isExpanded && (
        <div className="border-t border-zinc-100 px-4 pb-4 pt-3 dark:border-zinc-800">
          {/* Meta + Actions row */}
          <div className="flex items-center justify-between">
            <div className="text-xs text-zinc-500">
              {entry.ontology_name} v{entry.ontology_version_min}+
              <span className="mx-1.5 text-zinc-300 dark:text-zinc-700">·</span>
              Confidence <strong className="text-zinc-700 dark:text-zinc-300">{(entry.confidence * 100).toFixed(0)}%</strong>
              <span className="mx-1.5 text-zinc-300 dark:text-zinc-700">·</span>
              Used {entry.use_count} times
            </div>
            <div className="flex gap-1.5">
              {entry.status !== "approved" && (
                <button onClick={onApprove}
                  className="rounded-md bg-emerald-600 px-3 py-1 text-[11px] font-medium text-white hover:bg-emerald-700 transition">
                  Approve
                </button>
              )}
              {entry.status !== "deprecated" && (
                <button onClick={onDeprecate}
                  className="rounded-md border border-zinc-200 px-3 py-1 text-[11px] font-medium text-zinc-600 hover:bg-zinc-50 transition dark:border-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800">
                  Deprecate
                </button>
              )}
              <button onClick={onDelete}
                className="rounded-md border border-red-200 px-3 py-1 text-[11px] font-medium text-red-500 hover:bg-red-50 transition dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30">
                Delete
              </button>
            </div>
          </div>

          {/* Content */}
          <div className="mt-3 rounded-lg bg-zinc-50 p-4 dark:bg-zinc-950">
            <p className="whitespace-pre-wrap text-sm leading-relaxed text-zinc-700 dark:text-zinc-300">
              {entry.content}
            </p>
          </div>

          {/* Labels */}
          <div className="mt-3 flex flex-wrap gap-1.5">
            {entry.affected_labels.map((l) => (
              <span key={l} className="rounded-md bg-zinc-100 px-2 py-0.5 text-[11px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">
                {l}
              </span>
            ))}
          </div>

          {/* Structured data */}
          {entry.structured_data && Object.keys(entry.structured_data).length > 0 && (
            <details className="mt-3">
              <summary className="cursor-pointer text-[11px] text-zinc-400 hover:text-zinc-600">
                Structured data
              </summary>
              <pre className="mt-1.5 max-h-32 overflow-auto rounded-lg bg-zinc-950 p-3 text-[11px] text-emerald-400">
                {JSON.stringify(entry.structured_data, null, 2)}
              </pre>
            </details>
          )}

          {/* Footer meta */}
          <div className="mt-3 text-[10px] text-zinc-400">
            Created by {entry.created_by} · {new Date(entry.created_at).toLocaleString()}
            {entry.reviewed_at && <> · Reviewed {new Date(entry.reviewed_at).toLocaleString()}</>}
          </div>
        </div>
      )}
    </div>
  );
}

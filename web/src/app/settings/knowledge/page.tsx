"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import type { KnowledgeEntry, KnowledgeStatus } from "@/types/api";
import { bulkReviewKnowledge } from "@/lib/api/knowledge";
import { useAuth } from "@/lib/use-auth";
import { cn } from "@/lib/cn";
import {
  knowledgeKeys,
  useDeleteKnowledge,
  useKnowledgeInfinite,
  useUpdateKnowledgeStatus,
} from "@/hooks/api/use-knowledge";
import { useQueryClient } from "@tanstack/react-query";

const KIND_STYLES: Record<string, string> = {
  correction:
    "text-blue-700 bg-blue-50 ring-blue-600/20 dark:text-blue-400 dark:bg-blue-950/40 dark:ring-blue-400/20",
  hint: "text-violet-700 bg-violet-50 ring-violet-600/20 dark:text-violet-400 dark:bg-violet-950/40 dark:ring-violet-400/20",
};
const STATUS_DOT: Record<string, string> = {
  approved: "bg-emerald-500",
  draft: "bg-zinc-400",
  stale: "bg-amber-500",
  deprecated: "bg-zinc-300 dark:bg-zinc-600",
};

type KnownKind = "correction" | "hint";
type KnownStatusKey = "approved" | "draft" | "stale" | "deprecated";

function isKnownKind(k: string): k is KnownKind {
  return k === "correction" || k === "hint";
}
function isKnownStatus(s: string): s is KnownStatusKey {
  return s === "approved" || s === "draft" || s === "stale" || s === "deprecated";
}

const KB_PAGE_LIMIT = 100;

export default function KnowledgePage() {
  const t = useTranslations("settings.knowledge");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState("");
  const [kindFilter, setKindFilter] = useState("");
  const { isAdmin } = useAuth();
  const confirm = useConfirm();
  const qc = useQueryClient();

  const filters = {
    status: statusFilter || undefined,
    kind: kindFilter || undefined,
    limit: KB_PAGE_LIMIT,
  };

  const {
    data,
    isLoading,
    isError,
    hasNextPage,
    fetchNextPage,
    isFetchingNextPage,
  } = useKnowledgeInfinite(filters);

  useEffect(() => {
    if (isError) toast.error(t("toast.loadFailed"));
  }, [isError, t]);

  const entries = data?.pages.flatMap((p) => p.items) ?? [];

  const updateStatus = useUpdateKnowledgeStatus(filters);
  const deleteEntry = useDeleteKnowledge(filters);

  const statusLabel = (s: string) => (isKnownStatus(s) ? t(`status.${s}`) : s);

  const handleStatus = (id: string, status: KnowledgeStatus) => {
    updateStatus.mutate(
      { id, status },
      {
        onSuccess: () =>
          toast.success(t("toast.statusChanged", { status: statusLabel(status) })),
        onError: () => toast.error(t("toast.statusChangeFailed")),
      },
    );
  };

  const handleDelete = async (id: string) => {
    const entry = entries.find((e) => e.id === id);
    const ok = await confirm({
      title: t("deleteConfirmTitle", { title: entry?.title ?? id }),
      description: t("deleteConfirmDescription"),
      variant: "danger",
    });
    if (!ok) return;
    deleteEntry.mutate(id, {
      onSuccess: () => {
        if (expandedId === id) setExpandedId(null);
        toast.success(t("toast.deleted"));
      },
      onError: () => toast.error(t("toast.deleteFailed")),
    });
  };

  const staleCount = entries.filter((e) => e.status === "stale").length;

  if (!isAdmin) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t("adminOnly")}
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto">
        {/* Header */}
        <div className="flex items-start justify-between">
          <div>
            <h1 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
              {t("title")}
            </h1>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("description")}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {staleCount > 0 && (
              <button
                onClick={async () => {
                  const ids = entries.filter((e) => e.status === "stale").map((e) => e.id);
                  try {
                    await bulkReviewKnowledge(ids, "deprecated");
                    toast.success(t("toast.bulkDeprecated", { count: ids.length }));
                    qc.invalidateQueries({ queryKey: knowledgeKeys.lists() });
                  } catch {
                    toast.error(t("toast.bulkFailed"));
                  }
                }}
                className="rounded-md bg-amber-500 px-3 py-1.5 text-xs font-semibold text-white hover:bg-amber-600 transition"
              >
                {t("reviewStale", { count: staleCount })}
              </button>
            )}
          </div>
        </div>

        {/* Filters */}
        <div className="mt-4 flex items-center gap-3">
          <SettingsSelect
            label={t("statusFilterLabel")}
            hideLabel
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
          >
            <option value="">{t("allStatus")}</option>
            <option value="approved">{t("status.approved")}</option>
            <option value="draft">{t("status.draft")}</option>
            <option value="stale">{t("status.stale")}</option>
            <option value="deprecated">{t("status.deprecated")}</option>
          </SettingsSelect>
          <SettingsSelect
            label={t("kindFilterLabel")}
            hideLabel
            value={kindFilter}
            onChange={(e) => setKindFilter(e.target.value)}
          >
            <option value="">{t("allKinds")}</option>
            <option value="correction">{t("kind.correction")}</option>
            <option value="hint">{t("kind.hint")}</option>
          </SettingsSelect>
          <span className="ml-auto text-sm tabular-nums text-muted-foreground">
            {t("entries", { count: entries.length })}
          </span>
        </div>

        {/* Content */}
        <div className="mt-5">
          {isLoading ? (
            <div className="flex justify-center py-16"><Spinner /></div>
          ) : entries.length === 0 ? (
            <div className="rounded-xl border border-dashed border-zinc-300 px-6 py-16 text-center dark:border-zinc-700">
              <p className="text-sm text-muted-foreground">{t("empty")}</p>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("emptyHint")}
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

          {hasNextPage && !isLoading && entries.length > 0 && (
            <div className="mt-4 flex justify-center">
              <Button
                variant="outline"
                size="sm"
                onClick={() => fetchNextPage()}
                disabled={isFetchingNextPage}
              >
                {isFetchingNextPage
                  ? t("loading")
                  : t("loadMore", { count: entries.length })}
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function EntryCard({
  entry,
  isExpanded,
  onToggle,
  onApprove,
  onDeprecate,
  onDelete,
}: {
  entry: KnowledgeEntry;
  isExpanded: boolean;
  onToggle: () => void;
  onApprove: () => void;
  onDeprecate: () => void;
  onDelete: () => void;
}) {
  const t = useTranslations("settings.knowledge");
  const kindCls = KIND_STYLES[entry.kind] ?? KIND_STYLES.correction;
  const statusDot = STATUS_DOT[entry.status] ?? STATUS_DOT.draft;
  const kindLabel = isKnownKind(entry.kind)
    ? t(`kind.${entry.kind}`)
    : entry.kind;
  const statusLabel = isKnownStatus(entry.status)
    ? t(`status.${entry.status}`)
    : entry.status;

  return (
    <div
      className={cn(
        "rounded-xl border transition-all",
        isExpanded
          ? "border-emerald-200 bg-white shadow-sm dark:border-emerald-800/40 dark:bg-zinc-900"
          : "border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700",
      )}
    >
      <button onClick={onToggle} className="flex w-full items-center gap-3 px-4 py-3 text-left">
        <span className={cn("shrink-0 rounded-md px-2 py-0.5 text-[11px] font-semibold ring-1 ring-inset", kindCls)}>
          {kindLabel}
        </span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
          <span className={cn("h-2 w-2 rounded-full", statusDot)} />
          {statusLabel}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
          {entry.title}
        </span>
        <span className="hidden shrink-0 items-center gap-1 sm:flex">
          {entry.affected_labels.slice(0, 3).map((l) => (
            <span key={l} className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-zinc-500 dark:bg-zinc-800 dark:text-muted-foreground">{l}</span>
          ))}
          {entry.affected_labels.length > 3 && (
            <span className="text-[10px] text-muted-foreground">+{entry.affected_labels.length - 3}</span>
          )}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{(entry.confidence * 100).toFixed(0)}%</span>
        <svg
          className={cn("h-4 w-4 shrink-0 text-muted-foreground transition-transform", isExpanded && "rotate-180")}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {isExpanded && (
        <div className="border-t border-zinc-100 px-4 pb-4 pt-3 dark:border-zinc-800">
          <div className="flex items-center justify-between">
            <div className="text-xs text-muted-foreground">
              {t("ontologyMeta", {
                name: entry.ontology_name,
                version: entry.ontology_version_min,
              })}
              <span className="mx-1.5 text-zinc-300 dark:text-zinc-700">·</span>
              {t("confidence")}{" "}
              <strong className="text-zinc-700 dark:text-zinc-300">
                {(entry.confidence * 100).toFixed(0)}%
              </strong>
              <span className="mx-1.5 text-zinc-300 dark:text-zinc-700">·</span>
              {t("usedTimes", { count: entry.use_count })}
            </div>
            <div className="flex gap-1.5">
              {entry.status !== "approved" && (
                <button onClick={onApprove} className="rounded-md bg-emerald-600 px-3 py-1 text-[11px] font-medium text-white hover:bg-emerald-700 transition">
                  {t("approve")}
                </button>
              )}
              {entry.status !== "deprecated" && (
                <button onClick={onDeprecate} className="rounded-md border border-zinc-200 px-3 py-1 text-[11px] font-medium text-zinc-600 hover:bg-zinc-50 transition dark:border-zinc-700 dark:text-muted-foreground dark:hover:bg-zinc-800">
                  {t("deprecate")}
                </button>
              )}
              <button onClick={onDelete} className="rounded-md border border-red-200 px-3 py-1 text-[11px] font-medium text-red-500 hover:bg-red-50 transition dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30">
                {t("delete")}
              </button>
            </div>
          </div>

          <div className="mt-3 rounded-lg bg-zinc-50 p-4 dark:bg-zinc-950">
            <p className="whitespace-pre-wrap text-sm leading-relaxed text-zinc-700 dark:text-zinc-300">
              {entry.content}
            </p>
          </div>

          <div className="mt-3 flex flex-wrap gap-1.5">
            {entry.affected_labels.map((l) => (
              <span key={l} className="rounded-md bg-zinc-100 px-2 py-0.5 text-[11px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground">
                {l}
              </span>
            ))}
          </div>

          {entry.structured_data && Object.keys(entry.structured_data).length > 0 && (
            <details className="mt-3">
              <summary className="cursor-pointer text-[11px] text-muted-foreground hover:text-zinc-600">
                {t("structuredData")}
              </summary>
              <pre className="mt-1.5 max-h-32 overflow-auto rounded-lg bg-zinc-950 p-3 text-[11px] text-emerald-400">
                {JSON.stringify(entry.structured_data, null, 2)}
              </pre>
            </details>
          )}

          <div className="mt-3 text-[10px] text-muted-foreground">
            {t("createdBy", { user: entry.created_by })} ·{" "}
            {new Date(entry.created_at).toLocaleString()}
            {entry.reviewed_at && (
              <>
                {" "}
                ·{" "}
                {t("reviewed", {
                  date: new Date(entry.reviewed_at).toLocaleString(),
                })}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

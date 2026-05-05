"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "@/components/ui/toast";
import { ArrowDown01Icon, Book01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { SettingsSelect } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { useConfirm } from "@/components/providers/confirm-provider";
import type { KnowledgeEntry, KnowledgeStatus } from "@/types/api";
import { bulkReviewKnowledge } from "@/lib/api/knowledge";
import { useAuth } from "@/hooks/use-auth";
import { useFormatters } from "@/hooks/use-formatters";
import { cn } from "@/lib/cn";
import {
  knowledgeKeys,
  useDeleteKnowledge,
  useKnowledgeInfinite,
  useUpdateKnowledgeStatus,
} from "@/hooks/api/use-knowledge";

const KIND_STYLES: Record<string, string> = {
  correction:
    "text-info-foreground bg-info-surface ring-info-foreground",
  hint: "text-concept-foreground bg-concept-surface ring-concept-foreground/20",
};
const STATUS_DOT: Record<string, string> = {
  approved: "bg-brand-solid",
  draft: "bg-foreground-muted",
  stale: "bg-warning-foreground",
  deprecated: "bg-surface-raised",
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
  const t = useTranslations("settings.knowledge.base");
  const tCommon = useTranslations("common");
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
    refetch,
  } = useKnowledgeInfinite(filters);

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
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const headerActions = staleCount > 0 && (
    <Button
      variant="primary"
      size="sm"
      onClick={async () => {
        const ids = entries
          .filter((e) => e.status === "stale")
          .map((e) => e.id);
        try {
          await bulkReviewKnowledge(ids, "deprecated");
          toast.success(t("toast.bulkDeprecated", { count: ids.length }));
          qc.invalidateQueries({ queryKey: knowledgeKeys.lists() });
        } catch {
          toast.error(t("toast.bulkFailed"));
        }
      }}
    >
      {t("reviewStale", { count: staleCount })}
    </Button>
  );

  return (
    <SettingsPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={headerActions || undefined}
    >
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
          <span className="ms-auto text-sm tabular-nums text-foreground-muted">
            {t("entries", { count: entries.length })}
          </span>
        </div>

        {/* Content */}
        <div className="mt-5">
          <PageStateView
            state={
              (isLoading
                ? { kind: "loading" }
                : isError
                  ? { kind: "error", onRetry: () => void refetch() }
                  : entries.length === 0
                    ? { kind: "empty" }
                    : { kind: "data" }) satisfies PageState
            }
            skeleton={<SkeletonList count={5} />}
            empty={{
              icon: Book01Icon,
              title: t("empty"),
              description: t("emptyHint"),
            }}
            error={{
              title: tCommon("loadError.title"),
              description: tCommon("loadError.description"),
              retryLabel: tCommon("retry"),
            }}
          >
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
          </PageStateView>

          {hasNextPage && !isLoading && entries.length > 0 && (
            <div className="mt-4 flex justify-center">
              <Button
                variant="outline"
                size="sm"
                onClick={() => fetchNextPage()}
                disabled={isFetchingNextPage}
              >
                {isFetchingNextPage
                  ? tCommon("loading")
                  : t("loadMore", { count: entries.length })}
              </Button>
            </div>
          )}
        </div>
    </SettingsPageShell>
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
  const t = useTranslations("settings.knowledge.base");
  const tCommon = useTranslations("common");
  const fmt = useFormatters();
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
        "rounded-xl border transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
        isExpanded
          ? "border-brand-border bg-surface-base shadow-1"
          : "border-divider bg-surface-base hover:border-divider",
      )}
    >
      <button type="button" onClick={onToggle} className="flex w-full items-center gap-3 px-4 py-3 text-start">
        <span className={cn("shrink-0 rounded-md px-2 py-0.5 text-2xs font-semibold ring-1 ring-inset", kindCls)}>
          {kindLabel}
        </span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-foreground-muted">
          <span className={cn("h-2 w-2 rounded-full", statusDot)} />
          {statusLabel}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground-strong">
          {entry.title}
        </span>
        <span className="hidden shrink-0 items-center gap-1 sm:flex">
          {entry.affected_labels.slice(0, 3).map((l) => (
            <span key={l} className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs text-foreground-muted">{l}</span>
          ))}
          {entry.affected_labels.length > 3 && (
            <span className="text-2xs text-foreground-muted">+{entry.affected_labels.length - 3}</span>
          )}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-foreground-muted">{(entry.confidence * 100).toFixed(0)}%</span>
        <HugeiconsIcon
          icon={ArrowDown01Icon}
          className={cn(
            "h-4 w-4 shrink-0 text-foreground-muted transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)]",
            isExpanded && "rotate-180",
          )}
          size="100%"
        />
      </button>

      {isExpanded && (
        <div className="border-t border-divider-soft px-4 pb-4 pt-3">
          <div className="flex items-center justify-between">
            <div className="text-xs text-foreground-muted">
              {t("ontologyMeta", {
                name: entry.ontology_name,
                version: entry.ontology_version_min,
              })}
              <span className="mx-1.5 text-foreground-muted">·</span>
              {t("confidence")}{" "}
              <strong className="text-foreground">
                {(entry.confidence * 100).toFixed(0)}%
              </strong>
              <span className="mx-1.5 text-foreground-muted">·</span>
              {t("usedTimes", { count: entry.use_count })}
            </div>
            <div className="flex gap-1.5">
              {entry.status !== "approved" && (
                <Button variant="primary" size="xs" onClick={onApprove}>
                  {t("approve")}
                </Button>
              )}
              {entry.status !== "deprecated" && (
                <Button variant="outline" size="xs" onClick={onDeprecate}>
                  {t("deprecate")}
                </Button>
              )}
              <Button variant="danger" size="xs" onClick={onDelete}>
                {tCommon("delete")}
              </Button>
            </div>
          </div>

          <div className="mt-3 rounded-lg bg-surface-raised p-4">
            <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground">
              {entry.content}
            </p>
          </div>

          <div className="mt-3 flex flex-wrap gap-1.5">
            {entry.affected_labels.map((l) => (
              <span key={l} className="rounded-md bg-surface-inset px-2 py-0.5 text-2xs font-medium text-foreground">
                {l}
              </span>
            ))}
          </div>

          {entry.structured_data && Object.keys(entry.structured_data).length > 0 && (
            <details className="mt-3">
              <summary className="cursor-pointer text-2xs text-foreground-muted hover:text-foreground">
                {t("structuredData")}
              </summary>
              <pre className="mt-1.5 max-h-32 overflow-auto rounded-lg bg-surface-base p-3 text-2xs text-brand-foreground">
                {JSON.stringify(entry.structured_data, null, 2)}
              </pre>
            </details>
          )}

          <div className="mt-3 text-2xs text-foreground-muted">
            {t("createdBy", { user: entry.created_by })} ·{" "}
            {fmt.date(entry.created_at)}
            {entry.reviewed_at && (
              <>
                {" "}
                ·{" "}
                {t("reviewed", { date: fmt.date(entry.reviewed_at) })}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

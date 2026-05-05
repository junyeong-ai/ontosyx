"use client";

import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { ArrowDown01Icon, Book01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { Checkbox } from "@/components/ui/checkbox";
import { SettingsSelect } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { useConfirm } from "@/components/providers/confirm-provider";
import type { KnowledgeEntry, KnowledgeStatus } from "@/types/api";
import { useAuth } from "@/hooks/use-auth";
import { useFormatters } from "@/hooks/use-formatters";
import { cn } from "@/lib/cn";
import {
  useBulkReviewKnowledge,
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
  // Selected ids for the bulk-action bar. Set semantics give
  // toggle / has / size in O(1) without an array filter pass per
  // row render. Cleared whenever the filter changes (the user is
  // looking at a different result set so a stale selection would
  // be confusing).
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const { isAdmin } = useAuth();
  const confirm = useConfirm();

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
  const bulkReview = useBulkReviewKnowledge(filters);

  // Reset selection on filter change — the rows underneath have
  // changed, a leftover selection would silently keep ids from a
  // previous view.
  useEffect(() => {
    setSelectedIds(new Set());
  }, [statusFilter, kindFilter]);

  // Subset of the visible entries that are also selected. Drives
  // the "select-all" tri-state — checked when every visible row
  // is selected, indeterminate when some are.
  const selectedVisible = useMemo(
    () => entries.filter((e) => selectedIds.has(e.id)),
    [entries, selectedIds],
  );
  const allVisibleSelected =
    entries.length > 0 && selectedVisible.length === entries.length;
  const someVisibleSelected =
    selectedVisible.length > 0 && !allVisibleSelected;

  const toggleSelected = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const clearSelection = () => setSelectedIds(new Set());
  const toggleSelectAll = () => {
    if (allVisibleSelected) {
      clearSelection();
    } else {
      setSelectedIds(new Set(entries.map((e) => e.id)));
    }
  };

  const handleBulkStatus = (status: KnowledgeStatus) => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    bulkReview.mutate(
      { ids, status },
      {
        onSuccess: ({ reviewed }) => {
          clearSelection();
          toast.success(
            t(`toast.bulk.${status}` as const, { count: reviewed }),
          );
        },
        onError: () => toast.error(t("toast.bulkFailed")),
      },
    );
  };

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
      onClick={() => {
        const ids = entries
          .filter((e) => e.status === "stale")
          .map((e) => e.id);
        bulkReview.mutate(
          { ids, status: "deprecated" as KnowledgeStatus },
          {
            onSuccess: ({ reviewed }) =>
              toast.success(t("toast.bulkDeprecated", { count: reviewed })),
            onError: () => toast.error(t("toast.bulkFailed")),
          },
        );
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
            <>
              <div className="flex items-center gap-2 pb-2 ps-4 text-xs text-foreground-muted">
                <Checkbox
                  checked={allVisibleSelected}
                  ref={(el) => {
                    // tri-state visual: indeterminate when some
                    // (but not all) visible rows are selected.
                    if (el) el.indeterminate = someVisibleSelected;
                  }}
                  onChange={toggleSelectAll}
                  aria-label={t("selectAll")}
                />
                <span>{t("selectAllHint", { count: entries.length })}</span>
              </div>
              <div className="space-y-2">
                {entries.map((entry) => (
                  <EntryCard
                    key={entry.id}
                    entry={entry}
                    isExpanded={expandedId === entry.id}
                    selected={selectedIds.has(entry.id)}
                    onToggleSelect={() => toggleSelected(entry.id)}
                    onToggle={() =>
                      setExpandedId(expandedId === entry.id ? null : entry.id)
                    }
                    onApprove={() => handleStatus(entry.id, "approved")}
                    onDeprecate={() => handleStatus(entry.id, "deprecated")}
                    onDelete={() => handleDelete(entry.id)}
                  />
                ))}
              </div>
            </>
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

        <BulkActionBar
          count={selectedIds.size}
          onApprove={() => handleBulkStatus("approved")}
          onDeprecate={() => handleBulkStatus("deprecated")}
          onClear={clearSelection}
          pending={bulkReview.isPending}
        />
    </SettingsPageShell>
  );
}

function BulkActionBar({
  count,
  onApprove,
  onDeprecate,
  onClear,
  pending,
}: {
  count: number;
  onApprove: () => void;
  onDeprecate: () => void;
  onClear: () => void;
  pending: boolean;
}) {
  const t = useTranslations("settings.knowledge.base");
  const visible = count > 0;
  return (
    <div
      // Sticky bottom — slides into view on the first selection
      // and out on clear. `pointer-events-none` while hidden so a
      // mid-fade click can't trigger an action.
      className={cn(
        "pointer-events-none fixed inset-x-0 bottom-6 z-30 mx-auto flex max-w-2xl",
        "items-center justify-between gap-3 rounded-xl border border-divider",
        "bg-surface-overlay px-4 py-3 shadow-2",
        "transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
        visible ? "translate-y-0 opacity-100" : "pointer-events-none translate-y-4 opacity-0",
      )}
      role="region"
      aria-label={t("bulkBarLabel")}
      aria-hidden={!visible}
    >
      <span className="text-sm font-medium text-foreground-strong">
        {t("bulkSelectedCount", { count })}
      </span>
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={onClear}
          disabled={pending}
          className="pointer-events-auto"
        >
          {t("bulkClear")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={onDeprecate}
          disabled={pending}
          className="pointer-events-auto"
        >
          {t("bulkDeprecate")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onApprove}
          disabled={pending}
          className="pointer-events-auto"
        >
          {t("bulkApprove")}
        </Button>
      </div>
    </div>
  );
}

function EntryCard({
  entry,
  isExpanded,
  selected,
  onToggleSelect,
  onToggle,
  onApprove,
  onDeprecate,
  onDelete,
}: {
  entry: KnowledgeEntry;
  isExpanded: boolean;
  selected: boolean;
  onToggleSelect: () => void;
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
        "flex items-stretch rounded-xl border transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
        selected
          ? "border-brand-border bg-brand-surface/40"
          : isExpanded
            ? "border-brand-border bg-surface-base shadow-1"
            : "border-divider bg-surface-base hover:border-divider",
      )}
    >
      <div
        className="flex shrink-0 items-center pe-1 ps-4"
        // The checkbox sits OUTSIDE the toggle button so a click on
        // it never triggers the row's expand/collapse. Keyboard
        // focus reaches both — checkbox first (DOM order) then the
        // toggle button — so a screen-reader user can `Tab` →
        // Space to select without accidentally expanding the row.
        onClick={(e) => e.stopPropagation()}
      >
        <Checkbox
          checked={selected}
          onChange={onToggleSelect}
          aria-label={t("selectEntry", { title: entry.title })}
        />
      </div>
      <button type="button" onClick={onToggle} className="flex flex-1 items-center gap-3 px-3 py-3 text-start">
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

"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import Link from "next/link";

import {
  useCanonicalVersions,
  useDraftDiffAgainstCanonical,
  useRebaseDraft,
} from "@/hooks/api/use-ontology-branches";
import { useOntologyDrafts } from "@/hooks/api/use-ontology-drafts";
import { Button } from "@/components/ui/button";
import { Heading } from "@/components/ui/heading";
import { SkeletonList } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { cn } from "@/lib/cn";
import type { OntologyVersionEntry } from "@/types/ontology-branches";
import type { OntologyDraftSummary } from "@/types/ontology-drafts";

/**
 * Workspace × ontology = 1:1, so the branching tree is one
 * canonical lineage (committed versions, newest first) with
 * drafts hanging off each version via `parent_version_id`.
 *
 * The greenfield case (no canonical yet, drafts pre-first
 * commit) renders the drafts under a synthetic "no canonical"
 * trunk so the operator sees their work even before the first
 * version lands.
 */
function groupDraftsByParent(
  drafts: OntologyDraftSummary[],
): Map<string | null, OntologyDraftSummary[]> {
  const out = new Map<string | null, OntologyDraftSummary[]>();
  for (const d of drafts) {
    const key = d.parent_version_id ?? null;
    const list = out.get(key);
    if (list) {
      list.push(d);
    } else {
      out.set(key, [d]);
    }
  }
  // Stable ordering inside each bucket — newest first.
  for (const list of out.values()) {
    list.sort((a, b) =>
      a.updated_at < b.updated_at ? 1 : a.updated_at > b.updated_at ? -1 : 0,
    );
  }
  return out;
}

function formatTimestamp(value: string) {
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString();
}

function DraftDiffSummary({ draftId }: { draftId: string }) {
  const t = useTranslations("workbench.branches");
  const diff = useDraftDiffAgainstCanonical(draftId);
  if (diff.isLoading || !diff.data) return null;
  const total = diff.data.summary.total_changes;
  if (total === 0) {
    return (
      <span className="rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium text-foreground-muted">
        {t("diffNoChanges")}
      </span>
    );
  }
  return (
    <span className="rounded-full bg-info-surface px-2 py-0.5 text-2xs font-medium text-info-foreground">
      {t("diffSummary", {
        added: diff.data.summary.added_count,
        removed: diff.data.summary.removed_count,
        modified: diff.data.summary.modified_count,
      })}
    </span>
  );
}

function DraftActions({
  draft,
  isPinnedToHead,
}: {
  draft: OntologyDraftSummary;
  isPinnedToHead: boolean;
}) {
  const t = useTranslations("workbench.branches");
  const rebase = useRebaseDraft();
  const onRebase = () => {
    rebase.mutate(draft.id, {
      onSuccess: () => toast.success(t("rebaseSuccessToast")),
      onError: (err) =>
        toast.error(
          t("rebaseErrorToast", {
            error: err instanceof Error ? err.message : String(err),
          }),
        ),
    });
  };
  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onRebase();
      }}
      disabled={isPinnedToHead || rebase.isPending}
      loading={rebase.isPending}
      title={
        isPinnedToHead
          ? t("rebaseCurrentHint")
          : t("rebaseStaleHint")
      }
    >
      {rebase.isPending ? t("rebaseSubmitting") : t("rebaseLabel")}
    </Button>
  );
}

function VersionNode({
  version,
  drafts,
  draftLabel,
  currentHeadId,
}: {
  version: OntologyVersionEntry | null;
  drafts: OntologyDraftSummary[];
  draftLabel: string;
  currentHeadId: string | null;
}) {
  const t = useTranslations("workbench.branches");

  return (
    <li className="border-b border-divider last:border-b-0">
      <div className="flex items-baseline gap-3 px-4 py-3">
        <span
          aria-hidden
          className={cn(
            "inline-block h-2 w-2 shrink-0 rounded-full",
            version?.is_current
              ? "bg-success-foreground"
              : version
                ? "bg-foreground-muted"
                : "bg-warning-foreground",
          )}
        />
        <div className="min-w-0 flex-1">
          {version ? (
            <>
              <div className="flex items-baseline gap-2">
                <Heading level={3} size={6}>
                  {version.version}
                </Heading>
                {version.is_current ? (
                  <span className="rounded-full bg-success-surface px-2 py-0.5 text-2xs font-medium text-success-foreground">
                    {t("currentBadge")}
                  </span>
                ) : null}
              </div>
              <p className="mt-0.5 text-xs text-foreground-muted">
                {version.commit_message || t("emptyCommitMessage")} ·{" "}
                {version.committed_by} ·{" "}
                <span className="tabular-nums">
                  {formatTimestamp(version.created_at)}
                </span>
              </p>
            </>
          ) : (
            <>
              <Heading level={3} size={6}>
                {t("greenfieldTrunk")}
              </Heading>
              <p className="mt-0.5 text-xs text-foreground-muted">
                {t("greenfieldDescription")}
              </p>
            </>
          )}
        </div>
      </div>
      {drafts.length > 0 ? (
        <ul className="ml-6 border-l border-divider pl-4 pb-3">
          <li className="mb-1 text-2xs font-medium uppercase tracking-wide text-foreground-muted">
            {draftLabel} · {drafts.length}
          </li>
          {drafts.map((d) => {
            const pinnedToHead = !!(
              currentHeadId && d.parent_version_id === currentHeadId
            );
            return (
              <li
                key={d.id}
                className="rounded-lg px-3 py-1.5 hover:bg-surface-inset"
              >
                <div className="flex items-baseline justify-between gap-3">
                  <Link
                    href={`/projects/${d.id}`}
                    className="flex min-w-0 flex-1 items-baseline gap-2"
                  >
                    <span className="font-medium">
                      {d.title?.trim() ? d.title : t("untitledDraft")}
                    </span>
                    <span className="text-2xs text-foreground-muted">
                      {d.status}
                    </span>
                  </Link>
                  <div className="flex shrink-0 items-baseline gap-2">
                    <DraftDiffSummary draftId={d.id} />
                    <DraftActions draft={d} isPinnedToHead={pinnedToHead} />
                    <span className="text-2xs text-foreground-muted tabular-nums">
                      {formatTimestamp(d.updated_at)}
                    </span>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      ) : null}
    </li>
  );
}

export function BranchingTree() {
  const t = useTranslations("workbench.branches");
  const tCommon = useTranslations("common");
  const versionsQuery = useCanonicalVersions();
  const draftsQuery = useOntologyDrafts({ limit: 100 });

  const versions = versionsQuery.data?.versions ?? [];
  const drafts = draftsQuery.data?.items ?? [];
  const draftBuckets = useMemo(() => groupDraftsByParent(drafts), [drafts]);
  const greenfieldDrafts = draftBuckets.get(null) ?? [];
  const currentHeadId = versions.find((v) => v.is_current)?.id ?? null;

  const pageState: PageState =
    versionsQuery.isLoading || draftsQuery.isLoading
      ? { kind: "loading" }
      : versionsQuery.isError || draftsQuery.isError
        ? {
            kind: "error",
            onRetry: () => {
              if (versionsQuery.isError) void versionsQuery.refetch();
              if (draftsQuery.isError) void draftsQuery.refetch();
            },
          }
        : versions.length === 0 && drafts.length === 0
          ? { kind: "empty" }
          : { kind: "data" };

  return (
    <PageStateView
      state={pageState}
      skeleton={<SkeletonList count={4} />}
      error={{
        title: tCommon("loadError.title"),
        description: tCommon("loadError.description"),
        retryLabel: tCommon("retry"),
      }}
      empty={{
        title: t("emptyTitle"),
        description: t("emptyDescription"),
      }}
    >
      <div className="rounded-xl border border-divider">
        <ul>
          {versions.length === 0 && greenfieldDrafts.length > 0 ? (
            <VersionNode
              version={null}
              drafts={greenfieldDrafts}
              draftLabel={t("draftsLabel")}
              currentHeadId={currentHeadId}
            />
          ) : null}
          {versions.map((v) => (
            <VersionNode
              key={v.id}
              version={v}
              drafts={draftBuckets.get(v.id) ?? []}
              draftLabel={t("draftsLabel")}
              currentHeadId={currentHeadId}
            />
          ))}
          {versions.length > 0 && greenfieldDrafts.length > 0 ? (
            <VersionNode
              version={null}
              drafts={greenfieldDrafts}
              draftLabel={t("draftsLabel")}
              currentHeadId={currentHeadId}
            />
          ) : null}
        </ul>
      </div>
    </PageStateView>
  );
}

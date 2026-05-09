"use client";

import { useEffect } from "react";
import { useAppStore } from "@/lib/store";
import type { NotificationTone } from "@/lib/store/types";
import { useApprovals } from "@/hooks/api/use-approvals";
import { useStaleProposals } from "@/hooks/api/use-quality";
import { useAmbiguities } from "@/hooks/api/use-ambiguities";
import { useEvaluationRuns } from "@/hooks/api/use-evaluation";
import { useKnowledgeInfinite } from "@/hooks/api/use-knowledge";

interface ModeFeed {
  modeId: string;
  count: number;
  tone: NotificationTone;
}

/**
 * Workspace-wide notification feed. Reads the same queries each
 * operations page reads, then publishes the resulting per-mode
 * counts so the sidebar badges surface "you have N waiting"
 * without forcing the user to visit each page first.
 *
 * Counts are reconciled in a single effect so concurrent query
 * resolutions can never revert each other — every render
 * recomputes the full `feed` snapshot and applies it atomically.
 *
 * React Query handles the cache: every mounted page that runs the
 * same query reuses the same fetch, so this probe doesn't double
 * up network cost. Idle workspaces pay one round-trip per query
 * on mount, then nothing until staleTime expires.
 */
export function WorkspaceNotificationProbe() {
  const publish = useAppStore((s) => s.publishModeCount);
  const clear = useAppStore((s) => s.clearModeCount);

  const approvals = useApprovals();
  const stale = useStaleProposals(false);
  const ambiguities = useAmbiguities();
  const knowledge = useKnowledgeInfinite({ status: "stale", limit: 1 });
  const evaluation = useEvaluationRuns();

  const pendingApprovals = (approvals.data ?? []).filter(
    (a) => a.status === "pending",
  ).length;
  const stalePending = stale.data?.length ?? 0;
  const ambiguityPending =
    ambiguities.data?.items.filter((a) => !a.active_resolution).length ?? 0;
  const knowledgeStale =
    knowledge.data?.pages.flatMap((p) => p.items).length ?? 0;
  const failedRuns = (evaluation.data?.items ?? []).filter(
    (r) => r.status === "failed",
  ).length;

  useEffect(() => {
    const feed: ModeFeed[] = [
      { modeId: "approvals", count: pendingApprovals, tone: "warning" },
      {
        modeId: "quality",
        count: stalePending + ambiguityPending,
        tone: "warning",
      },
      { modeId: "knowledgeBase", count: knowledgeStale, tone: "warning" },
      { modeId: "evaluation", count: failedRuns, tone: "danger" },
    ];
    for (const entry of feed) {
      if (entry.count > 0) {
        publish(entry.modeId, { count: entry.count, tone: entry.tone });
      } else {
        clear(entry.modeId);
      }
    }
  }, [
    pendingApprovals,
    stalePending,
    ambiguityPending,
    knowledgeStale,
    failedRuns,
    publish,
    clear,
  ]);

  return null;
}

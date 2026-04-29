"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowRight01Icon,
  ChartUpIcon,
  CheckmarkCircle02Icon,
  PencilEdit01Icon,
} from "@hugeicons/core-free-icons";

import { useProjects } from "@/hooks/api/use-projects";
import { useAppStore } from "@/lib/store";
import { getProject } from "@/lib/api";
import { cn } from "@/lib/cn";
import type {
  DesignProjectSummary,
  DesignProjectStatus,
} from "@/types/api";

/**
 * Compact "resume work" list rendered below the create-project form.
 *
 * Pulls the most recent projects (up to `MAX_DISPLAY`) so a returning
 * operator can pick a project up where they left off rather than
 * scrolling through a separate listing page. Hidden entirely when the
 * list is empty (first-time users see only the create form), so the
 * empty state of the workspace stays focused on "create your first
 * project."
 *
 * Click → fetches the full `DesignProject` and pushes it into the
 * Zustand store (active project + ontology), which causes
 * `DesignPanel` to swap its view from the create form to the
 * project workflow without a page navigation.
 */
const MAX_DISPLAY = 5;

export function RecentProjects() {
  const t = useTranslations("workbench.bottomPanel.recentProjects");
  const { data, isLoading } = useProjects({ limit: MAX_DISPLAY });
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);

  const items = data?.items ?? [];
  if (isLoading) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-zinc-50/40 p-3 text-xs text-muted-foreground dark:border-zinc-800 dark:bg-zinc-900/40">
        {t("loading")}
      </div>
    );
  }
  if (items.length === 0) {
    return null;
  }

  const onResume = async (id: string) => {
    const project = await getProject(id);
    applyProjectSnapshot(project);
  };

  return (
    <section
      aria-label={t("ariaLabel")}
      className="rounded-lg border border-zinc-200 bg-zinc-50/40 dark:border-zinc-800 dark:bg-zinc-900/40"
    >
      <header className="flex items-center justify-between border-b border-zinc-200 px-3 py-2 dark:border-zinc-800">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("heading")}
        </h3>
        <span className="text-[10px] text-muted-foreground">
          {t("count", { count: items.length })}
        </span>
      </header>
      <ul className="divide-y divide-zinc-200 dark:divide-zinc-800">
        {items.map((p) => (
          <li key={p.id}>
            <button
              type="button"
              onClick={() => void onResume(p.id)}
              className="group flex w-full items-center gap-3 px-3 py-2 text-left transition-colors hover:bg-zinc-100 dark:hover:bg-zinc-800/60"
            >
              <StatusBadge status={p.status} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-xs font-medium text-zinc-800 dark:text-zinc-200">
                  {p.title ?? t("untitled")}
                </p>
                <p className="mt-0.5 truncate text-[10px] text-muted-foreground">
                  {t("meta", {
                    source: p.source_config.source_type,
                    updated: relativeTime(p.updated_at, t),
                  })}
                </p>
              </div>
              <HugeiconsIcon
                icon={ArrowRight01Icon}
                className="h-3 w-3 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
                size="100%"
              />
            </button>
          </li>
        ))}
      </ul>
      {/* ADR-0055 — link out to the full Project Hub for browsing
          beyond the compact 5-row "resume" list. */}
      <Link
        href="/projects"
        className="flex items-center justify-center gap-1 border-t border-zinc-200 px-3 py-2 text-[10px] font-medium text-emerald-700 transition-colors hover:bg-emerald-50/40 dark:border-zinc-800 dark:text-emerald-400 dark:hover:bg-emerald-950/20"
      >
        {t("viewAll")}
        <HugeiconsIcon icon={ArrowRight01Icon} className="h-3 w-3" size="100%" />
      </Link>
    </section>
  );
}

function StatusBadge({ status }: { status: DesignProjectStatus }) {
  const t = useTranslations("workbench.bottomPanel.recentProjects.status");
  const visual = {
    analyzed: {
      icon: ChartUpIcon,
      color:
        "bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300",
    },
    designed: {
      icon: PencilEdit01Icon,
      color:
        "bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300",
    },
    completed: {
      icon: CheckmarkCircle02Icon,
      color:
        "bg-blue-100 text-blue-700 dark:bg-blue-950/60 dark:text-blue-300",
    },
  } as const;
  const v = visual[status];
  return (
    <span
      className={cn(
        "flex h-7 w-7 shrink-0 items-center justify-center rounded-md",
        v.color,
      )}
      aria-label={t(status)}
      title={t(status)}
    >
      <HugeiconsIcon icon={v.icon} className="h-3.5 w-3.5" size="100%" />
    </span>
  );
}

/**
 * Minimal "X minutes/hours/days ago" formatter for the recent-list
 * meta line. Keeps the dependency surface free of a date-fns import
 * while still giving the operator a "freshness" signal.
 */
function relativeTime(
  iso: string,
  t: ReturnType<typeof useTranslations<"workbench.bottomPanel.recentProjects">>,
): string {
  const target = new Date(iso).getTime();
  if (Number.isNaN(target)) return iso;
  const diffMs = Date.now() - target;
  const minutes = Math.round(diffMs / 60_000);
  if (minutes < 1) return t("relativeJustNow");
  if (minutes < 60) return t("relativeMinutes", { count: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 24) return t("relativeHours", { count: hours });
  const days = Math.round(hours / 24);
  return t("relativeDays", { count: days });
}

/** A `<DesignProjectSummary>` with at least the fields RecentProjects reads. */
export type RecentProjectSummary = Pick<
  DesignProjectSummary,
  "id" | "status" | "title" | "source_config" | "updated_at"
>;

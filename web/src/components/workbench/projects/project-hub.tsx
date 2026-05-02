"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Add01Icon,
  ChartUpIcon,
  CheckmarkCircle02Icon,
  PencilEdit01Icon,
  Search01Icon,
} from "@hugeicons/core-free-icons";

import { useProjects } from "@/hooks/api/use-projects";
import { useAppStore } from "@/lib/store";
import { getProject } from "@/lib/api";
import { cn } from "@/lib/cn";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonCard } from "@/components/ui/skeleton";
import { Card } from "@/components/ui/card";
import type {
  DesignProjectStatus,
  DesignProjectSummary,
} from "@/types/api";

/**
 * Project Hub — card grid of every design project the operator
 * can see.
 *
 * Three surfaces:
 *
 * - **Search**: free-text match against `title` (case-insensitive,
 *   substring). Falls back to "Untitled" so projects without a
 *   title stay reachable via the empty-search default order.
 * - **Status filter**: `analyzed` / `designed` / `completed` chips
 *   that toggle a multi-select. Default is "all", reading the
 *   filter chips as muted; an active selection saturates them.
 * - **Card click**: hydrates the full `DesignProject` and pushes
 *   it through `applyProjectSnapshot` (the canonical project-mode
 *   entry point), then routes to `/design`. The transition keeps
 *   the active-project / ontology-cache invariant intact — same
 *   contract the recent-list `onResume` uses.
 *
 * The compact `<RecentProjects />` list inside `/design` continues
 * to serve "resume the project I just left." This page answers
 * "browse / pick across the whole workspace."
 */
export function ProjectHub() {
  const t = useTranslations("workbench.projects.hub");
  const tCommon = useTranslations("common");
  const router = useRouter();
  const projectsQuery = useProjects({ limit: 100 });
  const { data, isLoading, isError, refetch } = projectsQuery;
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);

  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<Set<DesignProjectStatus>>(
    new Set(),
  );

  const items = useMemo(() => data?.items ?? [], [data]);
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return items.filter((p) => {
      if (statusFilter.size > 0 && !statusFilter.has(p.status as DesignProjectStatus)) {
        return false;
      }
      if (q.length === 0) return true;
      const title = (p.title ?? "").toLowerCase();
      return title.includes(q);
    });
  }, [items, search, statusFilter]);

  const onOpen = async (id: string) => {
    const project = await getProject(id);
    applyProjectSnapshot(project);
    router.push("/design");
  };

  const onCreate = () => {
    // Push to /design — the design panel renders the create form
    // when no active project is set, which is exactly the entry
    // experience the hub's "new project" affordance should land on.
    router.push("/design");
  };

  const toggleStatus = (status: DesignProjectStatus) => {
    setStatusFilter((prev) => {
      const next = new Set(prev);
      if (next.has(status)) {
        next.delete(status);
      } else {
        next.add(status);
      }
      return next;
    });
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex shrink-0 items-center justify-between border-b border-divider px-6 py-4">
        <div>
          <h1 className="text-base font-semibold text-foreground-strong">
            {t("heading")}
          </h1>
          <p className="text-xs text-muted-foreground">
            {t("subheading", { count: items.length })}
          </p>
        </div>
        <button
          type="button"
          onClick={onCreate}
          className="flex items-center gap-1.5 rounded-md bg-brand-solid px-3 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-brand-solid-hover"
        >
          <HugeiconsIcon icon={Add01Icon} className="h-3.5 w-3.5" size="100%" />
          {t("createButton")}
        </button>
      </header>

      <div className="flex shrink-0 items-center gap-3 border-b border-divider px-6 py-3">
        <div className="relative flex-1 max-w-md">
          <HugeiconsIcon
            icon={Search01Icon}
            className="pointer-events-none absolute left-2 top-1 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
            size="100%"
          />
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("searchPlaceholder")}
            className="w-full rounded-md border border-divider bg-surface-base py-1.5 pl-7 pr-3 text-xs text-foreground-strong placeholder-muted-foreground focus:border-brand-border focus:outline-none-strong"
            aria-label={t("searchLabel")}
          />
        </div>
        <div className="flex items-center gap-1.5" role="group" aria-label={t("statusFilterLabel")}>
          {(["analyzed", "designed", "completed"] as const).map((status) => {
            const active = statusFilter.has(status);
            return (
              <button
                key={status}
                type="button"
                onClick={() => toggleStatus(status)}
                aria-pressed={active}
                className={cn(
                  "rounded-full border px-2.5 py-0.5 text-2xs font-medium transition-colors",
                  active
                    ? "border-brand-border bg-brand-surface text-brand-foreground-strong"
                    : "border-divider bg-surface-raised text-foreground hover:bg-surface-inset dark:text-muted-foreground dark:hover:bg-surface-base",
                )}
              >
                {t(`status.${status}`)}
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 py-6">
        {isLoading ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            {Array.from({ length: 8 }, (_, i) => (
              <SkeletonCard key={i} />
            ))}
          </div>
        ) : isError ? (
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => refetch()}
            retryLabel={tCommon("retry")}
          />
        ) : filtered.length === 0 ? (
          <EmptyState
            icon={Add01Icon}
            title={t("empty.heading")}
            description={t("empty.subheading")}
            action={{ label: t("empty.cta"), onClick: onCreate }}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            {filtered.map((p) => (
              <ProjectCard key={p.id} project={p} onOpen={() => void onOpen(p.id)} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function ProjectCard({
  project,
  onOpen,
}: {
  project: DesignProjectSummary;
  onOpen: () => void;
}) {
  const t = useTranslations("workbench.projects.hub");
  return (
    <Card
      variant="raised"
      interactive
      onClick={onOpen}
      className="flex flex-col items-stretch text-left"
    >
      <div className="flex items-center gap-2">
        <StatusIcon status={project.status as DesignProjectStatus} />
        <span className="truncate text-sm font-semibold text-foreground-strong">
          {project.title ?? t("untitled")}
        </span>
      </div>
      <p className="mt-3 truncate text-2xs text-foreground-muted">
        {t("cardMeta", {
          source: stringifySource(project.source_config),
          updated: relativeTime(project.updated_at, t),
        })}
      </p>
      <span className="mt-2 inline-flex w-fit items-center rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium uppercase tracking-wide text-foreground-muted">
        {t(`status.${project.status as DesignProjectStatus}`)}
      </span>
    </Card>
  );
}

function StatusIcon({ status }: { status: DesignProjectStatus }) {
  const visual = {
    analyzed: { icon: ChartUpIcon, color: "text-warning-foreground" },
    designed: { icon: PencilEdit01Icon, color: "text-brand-foreground" },
    completed: { icon: CheckmarkCircle02Icon, color: "text-info-foreground dark:text-info-foreground" },
  } as const;
  const v = visual[status];
  return (
    <HugeiconsIcon icon={v.icon} className={cn("h-4 w-4 shrink-0", v.color)} size="100%" />
  );
}

function stringifySource(source_config: unknown): string {
  if (typeof source_config === "object" && source_config !== null) {
    const cfg = source_config as { source_type?: string };
    if (typeof cfg.source_type === "string") return cfg.source_type;
  }
  return "—";
}

function relativeTime(
  iso: string,
  t: ReturnType<typeof useTranslations<"workbench.projects.hub">>,
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

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
  const router = useRouter();
  const { data, isLoading } = useProjects({ limit: 100 });
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
      <header className="flex shrink-0 items-center justify-between border-b border-zinc-200 px-6 py-4 dark:border-zinc-800">
        <div>
          <h1 className="text-base font-semibold text-zinc-800 dark:text-zinc-200">
            {t("heading")}
          </h1>
          <p className="text-xs text-muted-foreground">
            {t("subheading", { count: items.length })}
          </p>
        </div>
        <button
          type="button"
          onClick={onCreate}
          className="flex items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
        >
          <HugeiconsIcon icon={Add01Icon} className="h-3.5 w-3.5" size="100%" />
          {t("createButton")}
        </button>
      </header>

      <div className="flex shrink-0 items-center gap-3 border-b border-zinc-200 px-6 py-3 dark:border-zinc-800">
        <div className="relative flex-1 max-w-md">
          <HugeiconsIcon
            icon={Search01Icon}
            className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
            size="100%"
          />
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("searchPlaceholder")}
            className="w-full rounded-md border border-zinc-200 bg-white py-1.5 pl-7 pr-3 text-xs text-zinc-800 placeholder-muted-foreground focus:border-emerald-400 focus:outline-none dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
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
                  "rounded-full border px-2.5 py-0.5 text-[10px] font-medium transition-colors",
                  active
                    ? "border-emerald-400 bg-emerald-50 text-emerald-700 dark:border-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300"
                    : "border-zinc-200 bg-zinc-50 text-zinc-600 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-muted-foreground dark:hover:bg-zinc-800",
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
          <p className="text-xs text-muted-foreground">{t("loading")}</p>
        ) : filtered.length === 0 ? (
          <EmptyState onCreate={onCreate} />
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

function EmptyState({ onCreate }: { onCreate: () => void }) {
  const t = useTranslations("workbench.projects.hub");
  return (
    <div className="flex h-full flex-col items-center justify-center text-center">
      <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
        {t("empty.heading")}
      </p>
      <p className="mt-1 max-w-sm text-xs text-muted-foreground">
        {t("empty.subheading")}
      </p>
      <button
        type="button"
        onClick={onCreate}
        className="mt-4 flex items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
      >
        <HugeiconsIcon icon={Add01Icon} className="h-3.5 w-3.5" size="100%" />
        {t("empty.cta")}
      </button>
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
    <button
      type="button"
      onClick={onOpen}
      className="group flex flex-col items-stretch rounded-lg border border-zinc-200 bg-white p-4 text-left shadow-sm transition-colors hover:border-emerald-300 hover:bg-emerald-50/40 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-emerald-800 dark:hover:bg-emerald-950/20"
    >
      <div className="flex items-center gap-2">
        <StatusIcon status={project.status as DesignProjectStatus} />
        <span className="truncate text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          {project.title ?? t("untitled")}
        </span>
      </div>
      <p className="mt-3 truncate text-[10px] text-muted-foreground">
        {t("cardMeta", {
          source: stringifySource(project.source_config),
          updated: relativeTime(project.updated_at, t),
        })}
      </p>
      <span className="mt-2 inline-flex w-fit items-center rounded-full bg-zinc-100 px-2 py-0.5 text-[9px] font-medium uppercase tracking-wide text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground">
        {t(`status.${project.status as DesignProjectStatus}`)}
      </span>
    </button>
  );
}

function StatusIcon({ status }: { status: DesignProjectStatus }) {
  const visual = {
    analyzed: { icon: ChartUpIcon, color: "text-amber-600 dark:text-amber-400" },
    designed: { icon: PencilEdit01Icon, color: "text-emerald-700 dark:text-emerald-400" },
    completed: { icon: CheckmarkCircle02Icon, color: "text-blue-600 dark:text-blue-400" },
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

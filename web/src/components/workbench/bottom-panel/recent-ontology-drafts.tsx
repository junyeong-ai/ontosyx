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

import { useOntologyDrafts } from "@/hooks/api/use-ontology-drafts";
import { useAppStore } from "@/lib/store";
import { getOntologyDraft } from "@/lib/api";
import { cn } from "@/lib/cn";
import { Card } from "@/components/ui/card";
import type {
  OntologyDraftSummary,
  OntologyDraftStatus,
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
 * Click → fetches the full `OntologyDraft` and pushes it into the
 * Zustand store (active project + ontology), which causes
 * `DesignPanel` to swap its view from the create form to the
 * project workflow without a page navigation.
 */
const MAX_DISPLAY = 5;

export function RecentProjects() {
  const t = useTranslations("workbench.bottomPanel.recentProjects");
  const { data, isLoading } = useOntologyDrafts({ limit: MAX_DISPLAY });
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);

  const items = data?.items ?? [];
  if (isLoading) {
    return (
      <Card variant="inset" padding="sm" className="text-xs text-foreground-muted">
        {t("loading")}
      </Card>
    );
  }
  if (items.length === 0) {
    return null;
  }

  const onResume = async (id: string) => {
    const project = await getOntologyDraft(id);
    applyProjectSnapshot(project);
  };

  return (
    <Card
      role="region"
      padding="none"
      className="bg-surface-raised/40"
      aria-label={t("ariaLabel")}
    >
      <Card.Header className="px-3 py-2">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("heading")}
        </h3>
        <span className="text-2xs text-foreground-muted">
          {t("count", { count: items.length })}
        </span>
      </Card.Header>
      <ul className="divide-y divide-divider">
        {items.map((p) => (
          <li key={p.id}>
            <button
              type="button"
              onClick={() => void onResume(p.id)}
              className="group flex w-full items-center gap-3 px-3 py-2 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset"
            >
              <ProjectStatusIcon status={p.status} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-xs font-medium text-foreground-strong">
                  {p.title ?? t("untitled")}
                </p>
                <p className="mt-0.5 truncate text-2xs text-foreground-muted">
                  {t("meta", {
                    source: p.source_config.source_type,
                    updated: relativeTime(p.updated_at, t),
                  })}
                </p>
              </div>
              <HugeiconsIcon
                icon={ArrowRight01Icon}
                className="h-3 w-3 shrink-0 text-foreground-muted opacity-0 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] group-hover:opacity-100"
                size="100%"
              />
            </button>
          </li>
        ))}
      </ul>
      {/* Link out to the full Project Hub for browsing beyond
          the compact 5-row "resume" list. */}
      <Link
        href="/projects"
        className="flex items-center justify-center gap-1 border-t border-divider px-3 py-2 text-2xs font-medium text-brand-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-surface"
      >
        {t("viewAll")}
        <HugeiconsIcon icon={ArrowRight01Icon} className="h-3 w-3" size="100%" />
      </Link>
    </Card>
  );
}

const STATUS_VISUAL: Record<OntologyDraftStatus, { icon: typeof ChartUpIcon; tone: "warning" | "success" | "info" }> = {
  analyzed:  { icon: ChartUpIcon,            tone: "warning" },
  designed:  { icon: PencilEdit01Icon,       tone: "success" },
  completed: { icon: CheckmarkCircle02Icon,  tone: "info" },
};

function ProjectStatusIcon({ status }: { status: OntologyDraftStatus }) {
  const t = useTranslations("workbench.bottomPanel.recentProjects.status");
  const { icon, tone } = STATUS_VISUAL[status];
  const toneClass =
    tone === "warning" ? "bg-warning-surface text-warning-foreground"
    : tone === "success" ? "bg-success-surface text-success-foreground"
    : "bg-info-surface text-info-foreground";
  return (
    <span
      className={cn("flex h-7 w-7 shrink-0 items-center justify-center rounded-md", toneClass)}
      aria-label={t(status)}
      title={t(status)}
    >
      <HugeiconsIcon icon={icon} className="h-3.5 w-3.5" size="100%" />
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

/** A `<OntologyDraftSummary>` with at least the fields RecentProjects reads. */
export type RecentProjectSummary = Pick<
  OntologyDraftSummary,
  "id" | "status" | "title" | "source_config" | "updated_at"
>;

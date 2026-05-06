"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { ArrowRight, Pencil } from "lucide-react";
import { CheckCircle2, TrendingUp } from "lucide-react";
import { useOntologyDrafts } from "@/hooks/api/use-ontology-drafts";
import { useAppStore } from "@/lib/store";
import { getOntologyDraft } from "@/lib/api";
import { cn } from "@/lib/cn";
import { Card } from "@/components/ui/card";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import { Eyebrow } from "@/components/ui/eyebrow";
import type {
  OntologyDraftSummary,
  OntologyDraftStatus,
} from "@/types/api";

/**
 * Compact "resume work" list rendered below the create-draft form.
 *
 * Pulls the most recent drafts (up to `MAX_DISPLAY`) so a returning
 * operator can pick one up where they left off rather than scrolling
 * through a separate listing page. Hidden entirely when the list is
 * empty (first-time users see only the create form), so the empty
 * state of the workspace stays focused on "create your first draft."
 *
 * Click → fetches the full `OntologyDraft` and pushes it into the
 * Zustand store (active draft + ontology), which causes `DesignPanel`
 * to swap its view from the create form to the draft workflow
 * without a page navigation.
 */
const MAX_DISPLAY = 5;

export function RecentOntologyDrafts() {
  const t = useTranslations("workbench.bottomPanel.recentOntologyDrafts");
  const { data, isLoading } = useOntologyDrafts({ limit: MAX_DISPLAY });
  const applyOntologyDraftSnapshot = useAppStore((s) => s.applyOntologyDraftSnapshot);

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
    const draft = await getOntologyDraft(id);
    applyOntologyDraftSnapshot(draft);
  };

  return (
    <Card
      role="region"
      padding="none"
      className="bg-surface-raised/40"
      aria-label={t("ariaLabel")}
    >
      <Card.Header className="px-3 py-2">
        <Eyebrow level={3} size="dense">
          {t("heading")}
        </Eyebrow>
        <span className="text-2xs text-foreground-muted">
          {t("count", { count: items.length })}
        </span>
      </Card.Header>
      <ul className="divide-y divide-divider">
        {items.map((d) => (
          <li key={d.id}>
            <button
              type="button"
              onClick={() => void onResume(d.id)}
              className="group flex w-full items-center gap-3 px-3 py-2 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset"
            >
              <OntologyDraftStatusIcon status={d.status} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-xs font-medium text-foreground-strong">
                  {d.title ?? t("untitled")}
                </p>
                <p className="mt-0.5 truncate text-2xs text-foreground-muted">
                  {t("meta", {
                    source: d.source_config.source_type,
                    updated: relativeTime(d.updated_at, t),
                  })}
                </p>
              </div>
              <ArrowRight className="h-3 w-3 shrink-0 text-foreground-muted opacity-0 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] group-hover:opacity-100" />
            </button>
          </li>
        ))}
      </ul>
      {/* Link out to the full Ontology Draft Hub for browsing beyond
          the compact 5-row "resume" list. */}
      <Link
        href="/ontology-drafts"
        className="flex items-center justify-center gap-1 border-t border-divider px-3 py-2 text-2xs font-medium text-brand-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-surface"
      >
        {t("viewAll")}
        <ArrowRight className="h-3 w-3" />
      </Link>
    </Card>
  );
}

const STATUS_VISUAL: Record<OntologyDraftStatus, { icon: typeof TrendingUp; tone: "warning" | "success" | "info" }> = {
  analyzed:  { icon: TrendingUp,            tone: "warning" },
  designed:  { icon: Pencil,       tone: "success" },
  completed: { icon: CheckCircle2,  tone: "info" },
};

function OntologyDraftStatusIcon({ status }: { status: OntologyDraftStatus }) {
  const t = useTranslations("workbench.bottomPanel.recentOntologyDrafts.status");
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
      <DynamicIcon as={icon} className="h-3.5 w-3.5" />
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
  t: ReturnType<typeof useTranslations<"workbench.bottomPanel.recentOntologyDrafts">>,
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

/** A `<OntologyDraftSummary>` with at least the fields the recent list reads. */
export type RecentOntologyDraftSummary = Pick<
  OntologyDraftSummary,
  "id" | "status" | "title" | "source_config" | "updated_at"
>;

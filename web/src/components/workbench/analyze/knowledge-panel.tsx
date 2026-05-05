"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { listKnowledge } from "@/lib/api/knowledge";
import type { KnowledgeEntry } from "@/types/api";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/cn";

const STATUS_DOT: Record<string, string> = {
  approved: "bg-brand-solid",
  draft: "bg-foreground-muted",
  stale: "bg-warning-foreground",
  deprecated: "bg-surface-raised",
};

export function KnowledgePanel() {
  const t = useTranslations("workbench.analyze.knowledgePanel");
  const ontology = useAppStore((s) => s.ontology);
  const [entries, setEntries] = useState<KnowledgeEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const ontologyName = ontology?.name;

  useEffect(() => {
    if (!ontologyName) return;
    let cancelled = false;
    const load = async () => {
      try {
        const page = await listKnowledge({ ontology_name: ontologyName, status: "approved", limit: 50 });
        if (!cancelled) setEntries(page.items);
      } catch {
        if (!cancelled) setEntries([]);
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    load();
    return () => { cancelled = true; };
  }, [ontologyName]);

  if (!ontologyName) {
    return (
      <div className="flex h-full items-center justify-center text-xs text-foreground-muted">
        {t("loadHint")}
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (entries.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-xs text-foreground-muted">
        <p>{t("empty.title")}</p>
        <p className="text-2xs">
          {t("empty.description")}
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto divide-y divide-divider-soft">
        {entries.map((entry) => (
          <button type="button"
            key={entry.id}
            onClick={() => setSelectedId(selectedId === entry.id ? null : entry.id)}
            className={cn(
              "w-full px-3 py-2 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              selectedId === entry.id
                ? "bg-brand-surface/20"
                : "hover:bg-surface-raised",
            )}
          >
            <div className="flex items-center gap-2">
              <span className={cn("h-1.5 w-1.5 rounded-full", STATUS_DOT[entry.status] ?? STATUS_DOT.draft)} />
              <span className="text-2xs font-medium text-foreground-muted uppercase">{entry.kind}</span>
              <span className="ms-auto text-2xs tabular-nums text-foreground-muted">
                {(entry.confidence * 100).toFixed(0)}%
              </span>
            </div>
            <p className="mt-0.5 text-xs text-foreground line-clamp-2">
              {entry.title}
            </p>
            {selectedId === entry.id && (
              <div className="mt-2 rounded border border-divider bg-surface-raised p-2 text-2xs text-foreground">
                {entry.content}
              </div>
            )}
          </button>
        ))}
      </div>
      <div className="shrink-0 border-t border-divider px-3 py-1.5 text-2xs text-foreground-muted">
        {t("footer", { count: entries.length, ontologyName })}
      </div>
    </div>
  );
}

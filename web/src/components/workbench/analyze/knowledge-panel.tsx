"use client";

import { useEffect, useState } from "react";
import { useAppStore } from "@/lib/store";
import { listKnowledge } from "@/lib/api/knowledge";
import type { KnowledgeEntry } from "@/types/api";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/cn";

const STATUS_DOT: Record<string, string> = {
  approved: "bg-brand-solid",
  draft: "bg-muted-foreground",
  stale: "bg-warning-foreground",
  deprecated: "bg-surface-raised",
};

export function KnowledgePanel() {
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
      <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
        Load an ontology to view knowledge
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
      <div className="flex h-full flex-col items-center justify-center gap-2 text-xs text-muted-foreground">
        <p>No knowledge entries for this ontology.</p>
        <p className="text-2xs">
          Entries are auto-created when query translation fails, or manually via Settings &gt; Knowledge.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto divide-y divide-divider-soft">
        {entries.map((entry) => (
          <button
            key={entry.id}
            onClick={() => setSelectedId(selectedId === entry.id ? null : entry.id)}
            className={cn(
              "w-full px-3 py-2 text-left transition-colors",
              selectedId === entry.id
                ? "bg-brand-surface/20"
                : "hover:bg-surface-raised dark:hover:bg-surface-base/50",
            )}
          >
            <div className="flex items-center gap-2">
              <span className={cn("h-1.5 w-1.5 rounded-full", STATUS_DOT[entry.status] ?? STATUS_DOT.draft)} />
              <span className="text-2xs font-medium text-muted-foreground uppercase">{entry.kind}</span>
              <span className="ml-auto text-2xs tabular-nums text-muted-foreground">
                {(entry.confidence * 100).toFixed(0)}%
              </span>
            </div>
            <p className="mt-0.5 text-xs text-foreground line-clamp-2">
              {entry.title}
            </p>
            {selectedId === entry.id && (
              <div className="mt-2 rounded border border-divider bg-surface-raised p-2 text-[11px] text-foreground dark:text-muted-foreground">
                {entry.content}
              </div>
            )}
          </button>
        ))}
      </div>
      <div className="shrink-0 border-t border-divider px-3 py-1.5 text-2xs text-muted-foreground">
        {entries.length} entries · {ontologyName}
      </div>
    </div>
  );
}

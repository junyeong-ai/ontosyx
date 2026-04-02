"use client";

import { useEffect, useState } from "react";
import { useAppStore } from "@/lib/store";
import { listKnowledge } from "@/lib/api/knowledge";
import type { KnowledgeEntry } from "@/types/api";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/cn";

const STATUS_DOT: Record<string, string> = {
  approved: "bg-emerald-500",
  draft: "bg-zinc-400",
  stale: "bg-amber-500",
  deprecated: "bg-zinc-300",
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
      <div className="flex h-full items-center justify-center text-xs text-zinc-400">
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
      <div className="flex h-full flex-col items-center justify-center gap-2 text-xs text-zinc-400">
        <p>No knowledge entries for this ontology.</p>
        <p className="text-[10px]">
          Entries are auto-created when query translation fails, or manually via Settings &gt; Knowledge.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto divide-y divide-zinc-100 dark:divide-zinc-800">
        {entries.map((entry) => (
          <button
            key={entry.id}
            onClick={() => setSelectedId(selectedId === entry.id ? null : entry.id)}
            className={cn(
              "w-full px-3 py-2 text-left transition-colors",
              selectedId === entry.id
                ? "bg-emerald-50 dark:bg-emerald-950/20"
                : "hover:bg-zinc-50 dark:hover:bg-zinc-800/50",
            )}
          >
            <div className="flex items-center gap-2">
              <span className={cn("h-1.5 w-1.5 rounded-full", STATUS_DOT[entry.status] ?? STATUS_DOT.draft)} />
              <span className="text-[10px] font-medium text-zinc-500 uppercase">{entry.kind}</span>
              <span className="ml-auto text-[9px] tabular-nums text-zinc-400">
                {(entry.confidence * 100).toFixed(0)}%
              </span>
            </div>
            <p className="mt-0.5 text-xs text-zinc-700 dark:text-zinc-300 line-clamp-2">
              {entry.title}
            </p>
            {selectedId === entry.id && (
              <div className="mt-2 rounded border border-zinc-200 bg-zinc-50 p-2 text-[11px] text-zinc-600 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400">
                {entry.content}
              </div>
            )}
          </button>
        ))}
      </div>
      <div className="shrink-0 border-t border-zinc-200 px-3 py-1.5 text-[10px] text-zinc-400 dark:border-zinc-700">
        {entries.length} entries · {ontologyName}
      </div>
    </div>
  );
}

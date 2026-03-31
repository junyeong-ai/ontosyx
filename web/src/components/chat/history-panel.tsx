"use client";

import { useCallback, useEffect, useState } from "react";
import {
  listExecutions,
  getExecution,
  listPins,
  deletePin,
} from "@/lib/api";
import type {
  QueryExecutionSummary,
  QueryExecution,
  PinboardItem,
} from "@/types/api";
import { Button } from "@/components/ui/button";
import { Tabs } from "@base-ui/react/tabs";
import { HugeiconsIcon } from "@hugeicons/react";
import { EmptyState } from "@/components/ui/empty-state";
import {
  Clock01Icon,
  PinIcon,
  Delete01Icon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { SkeletonList } from "@/components/ui/skeleton";
import { toast } from "sonner";
import { ExecutionDetail } from "@/components/chat/execution-detail";
import { ExecutionCard } from "@/components/chat/execution-card";

type Tab = "recent" | "pinned";

export function HistoryPanel() {
  const [tab, setTab] = useState<Tab>("recent");
  const [refreshKey, setRefreshKey] = useState(0);

  const handleTabChange = (value: Tab | null) => {
    if (!value) return;
    setTab(value);
    setRefreshKey((k) => k + 1);
  };

  return (
    <div className="flex h-full flex-col bg-zinc-50/50 dark:bg-zinc-950">
      <Tabs.Root value={tab} onValueChange={handleTabChange}>
        {/* Tab bar */}
        <Tabs.List className="flex border-b border-zinc-200 dark:border-zinc-800">
          <Tabs.Tab
            value="recent"
            className="flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium outline-none transition-colors text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300 data-[active]:border-b-2 data-[active]:border-emerald-600 data-[active]:text-emerald-700 dark:data-[active]:text-emerald-400"
          >
            <HugeiconsIcon icon={Clock01Icon} className="h-3.5 w-3.5" size="100%" />
            Recent
          </Tabs.Tab>
          <Tabs.Tab
            value="pinned"
            className="flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium outline-none transition-colors text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300 data-[active]:border-b-2 data-[active]:border-emerald-600 data-[active]:text-emerald-700 dark:data-[active]:text-emerald-400"
          >
            <HugeiconsIcon icon={PinIcon} className="h-3.5 w-3.5" size="100%" />
            Pinned
          </Tabs.Tab>
        </Tabs.List>

        {/* Content */}
        <div className="flex-1 overflow-y-auto">
          <Tabs.Panel value="recent">
            <RecentTab key={refreshKey} />
          </Tabs.Panel>
          <Tabs.Panel value="pinned">
            <PinnedTab key={refreshKey} />
          </Tabs.Panel>
        </div>
      </Tabs.Root>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recent Tab — query execution history
// ---------------------------------------------------------------------------

function RecentTab() {
  const [items, setItems] = useState<QueryExecutionSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | undefined>();
  const [loading, setLoading] = useState(true);
  const [detail, setDetail] = useState<QueryExecution | null>(null);

  const loadPage = useCallback(async (cursor?: string) => {
    setLoading(true);
    try {
      const page = await listExecutions({ cursor, limit: 20 });
      if (cursor) {
        setItems((prev) => [...prev, ...page.items]);
      } else {
        setItems(page.items);
      }
      setNextCursor(page.next_cursor);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load history");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadPage();
  }, [loadPage]);

  if (detail) {
    return <ExecutionDetail execution={detail} onBack={() => setDetail(null)} />;
  }

  return (
    <div className="p-4">
      {items.length === 0 && loading && <SkeletonList count={4} />}

      {items.length === 0 && !loading && (
        <EmptyState icon={Clock01Icon} title="No query executions yet" />
      )}

      <div className="space-y-2">
        {items.map((item) => (
          <ExecutionCard
            key={item.id}
            item={item}
            onClick={async () => {
              try {
                const full = await getExecution(item.id);
                setDetail(full);
              } catch (err) {
                toast.error(err instanceof Error ? err.message : "Failed to load execution");
              }
            }}
          />
        ))}
      </div>

      {loading && (
        <div className="flex justify-center py-8">
          <Spinner size="md" className="text-zinc-400" />
        </div>
      )}

      {nextCursor && !loading && (
        <div className="pt-3 text-center">
          <Button variant="ghost" size="sm" onClick={() => loadPage(nextCursor)}>
            Load more
          </Button>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pinned Tab — pinboard items
// ---------------------------------------------------------------------------

function PinnedTab() {
  const [items, setItems] = useState<PinboardItem[]>([]);
  const [nextCursor, setNextCursor] = useState<string | undefined>();
  const [loading, setLoading] = useState(true);
  const [detail, setDetail] = useState<QueryExecution | null>(null);

  const loadPage = useCallback(async (cursor?: string) => {
    setLoading(true);
    try {
      const page = await listPins({ cursor, limit: 20 });
      if (cursor) {
        setItems((prev) => [...prev, ...page.items]);
      } else {
        setItems(page.items);
      }
      setNextCursor(page.next_cursor);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to load pins");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadPage();
  }, [loadPage]);

  const handleUnpin = async (id: string) => {
    try {
      await deletePin(id);
      setItems((prev) => prev.filter((p) => p.id !== id));
      toast.success("Unpinned");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to unpin");
    }
  };

  if (detail) {
    return <ExecutionDetail execution={detail} onBack={() => setDetail(null)} />;
  }

  return (
    <div className="p-4">
      {items.length === 0 && !loading && (
        <EmptyState icon={PinIcon} title="No pinned results yet" />
      )}

      <div className="space-y-2">
        {items.map((item) => (
          <div
            key={item.id}
            className="group flex items-start gap-2 rounded-lg border border-zinc-200 bg-white p-3 dark:border-zinc-800 dark:bg-zinc-900"
          >
            <button
              onClick={async () => {
                try {
                  const full = await getExecution(item.query_execution_id);
                  setDetail(full);
                } catch (err) {
                  toast.error(
                    err instanceof Error ? err.message : "Failed to load execution",
                  );
                }
              }}
              aria-label={`View pinned: ${item.title ?? "Untitled"}`}
              className="min-w-0 flex-1 text-left"
            >
              <p className="text-sm font-medium text-zinc-800 dark:text-zinc-200 line-clamp-2">
                {item.title ?? "Untitled pin"}
              </p>
              <p className="mt-1 text-xs text-zinc-400">
                {new Date(item.pinned_at).toLocaleString(undefined, {
                  month: "short",
                  day: "numeric",
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </p>
            </button>
            <button
              onClick={() => handleUnpin(item.id)}
              className="rounded p-1 text-zinc-300 opacity-0 transition-all hover:bg-red-50 hover:text-red-500 group-hover:opacity-100 group-focus-within:opacity-100 dark:hover:bg-red-900/20"
              aria-label="Unpin"
            >
              <HugeiconsIcon icon={Delete01Icon} className="h-3.5 w-3.5" size="100%" />
            </button>
          </div>
        ))}
      </div>

      {loading && (
        <div className="flex justify-center py-8">
          <Spinner size="md" className="text-zinc-400" />
        </div>
      )}

      {nextCursor && !loading && (
        <div className="pt-3 text-center">
          <Button variant="ghost" size="sm" onClick={() => loadPage(nextCursor)}>
            Load more
          </Button>
        </div>
      )}
    </div>
  );
}

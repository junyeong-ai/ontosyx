"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
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
import { TabBar } from "@/components/ui/tab-bar";
import { EmptyState } from "@/components/ui/empty-state";
import { Clock, Trash2 } from "lucide-react";
import { Pin } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { SkeletonList } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { ExecutionDetail } from "@/components/chat/execution-detail";
import { ExecutionCard } from "@/components/chat/execution-card";
import { useFormatters } from "@/hooks/use-formatters";

type Tab = "recent" | "pinned";

export function HistoryPanel() {
  const t = useTranslations("workbench.chat.history");
  const [tab, setTab] = useState<Tab>("recent");
  const [refreshKey, setRefreshKey] = useState(0);

  const handleTabChange = (value: string) => {
    setTab(value as Tab);
    setRefreshKey((k) => k + 1);
  };

  return (
    <div className="flex h-full flex-col bg-surface-raised">
      <div className="border-b border-divider">
        <TabBar
          tabs={[
            { id: "recent", label: t("tabRecent"), icon: Clock },
            { id: "pinned", label: t("tabPinned"), icon: Pin },
          ]}
          activeTab={tab}
          onTabChange={handleTabChange}
        />
      </div>

      <div className="flex-1 overflow-y-auto">
        {tab === "recent" && <RecentTab key={refreshKey} />}
        {tab === "pinned" && <PinnedTab key={refreshKey} />}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recent Tab — query execution history
// ---------------------------------------------------------------------------

function RecentTab() {
  const t = useTranslations("workbench.chat.history");
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
      toast.error(err instanceof Error ? err.message : t("toast.loadHistoryFailed"));
    } finally {
      setLoading(false);
    }
  }, [t]);

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
        <EmptyState icon={Clock} title={t("emptyRecent")} />
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
                toast.error(err instanceof Error ? err.message : t("toast.loadExecutionFailed"));
              }
            }}
          />
        ))}
      </div>

      {loading && (
        <div className="flex justify-center py-8">
          <Spinner size="md" className="text-foreground-muted" />
        </div>
      )}

      {nextCursor && !loading && (
        <div className="pt-3 text-center">
          <Button variant="ghost" size="sm" onClick={() => loadPage(nextCursor)}>
            {t("loadMore")}
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
  const t = useTranslations("workbench.chat.history");
  const fmt = useFormatters();
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
      toast.error(err instanceof Error ? err.message : t("toast.loadPinsFailed"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadPage();
  }, [loadPage]);

  const handleUnpin = async (id: string) => {
    try {
      await deletePin(id);
      setItems((prev) => prev.filter((p) => p.id !== id));
      toast.success(t("toast.unpinned"));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("unpinFailed"));
    }
  };

  if (detail) {
    return <ExecutionDetail execution={detail} onBack={() => setDetail(null)} />;
  }

  return (
    <div className="p-4">
      {items.length === 0 && !loading && (
        <EmptyState icon={Pin} title={t("emptyPinned")} />
      )}

      <div className="space-y-2">
        {items.map((item) => (
          <div
            key={item.id}
            className="group flex items-start gap-2 rounded-lg border border-divider bg-surface-base p-3"
          >
            <button
              type="button"
              onClick={async () => {
                try {
                  const full = await getExecution(item.query_execution_id);
                  setDetail(full);
                } catch (err) {
                  toast.error(
                    err instanceof Error ? err.message : t("toast.loadExecutionFailed"),
                  );
                }
              }}
              aria-label={t("viewPinnedAria", { title: item.title ?? t("untitledPin") })}
              className="min-w-0 flex-1 text-start"
            >
              <p className="text-sm font-medium text-foreground-strong line-clamp-2">
                {item.title ?? t("untitledPin")}
              </p>
              <p className="mt-1 text-xs text-foreground-muted">
                {fmt.date(item.pinned_at, {
                  month: "short",
                  day: "numeric",
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </p>
            </button>
            <button
              type="button"
              onClick={() => handleUnpin(item.id)}
              className="rounded p-1 text-foreground-muted opacity-0 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] hover:bg-danger-surface hover:text-danger-foreground group-hover:opacity-100 group-focus-within:opacity-100"
              aria-label={t("unpinAria")}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </div>

      {loading && (
        <div className="flex justify-center py-8">
          <Spinner size="md" className="text-foreground-muted" />
        </div>
      )}

      {nextCursor && !loading && (
        <div className="pt-3 text-center">
          <Button variant="ghost" size="sm" onClick={() => loadPage(nextCursor)}>
            {t("loadMore")}
          </Button>
        </div>
      )}
    </div>
  );
}

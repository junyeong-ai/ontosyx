"use client";

import { useEffect, useMemo, useState } from "react";
import { z } from "zod";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { FormInput, SettingsSelect } from "@/components/ui/form-input";
import { useQueryState } from "@/hooks/use-query-state";
import { useImeAwareInput } from "@/lib/use-ime-aware-input";
import { Button } from "@/components/ui/button";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { toast } from "sonner";
import type { AgentSession, AgentEvent, SessionMessage } from "@/types/api";
import {
  listAgentSessions,
  listAgentEvents,
  fetchSessionMessages,
  deleteSession,
} from "@/lib/api";

const PAGE_LIMIT = 50;

function StatCard({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="rounded-lg border border-zinc-200 bg-white px-4 py-3 dark:border-zinc-800 dark:bg-zinc-900">
      <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-zinc-900 dark:text-zinc-100">{value}</p>
    </div>
  );
}

export default function SessionsPage() {
  const t = useTranslations("settings.sessions");
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [eventsLoading, setEventsLoading] = useState(false);
  const [search, setSearch] = useQueryState("q", {
    default: "",
    parser: z.string(),
  });
  const searchInput = useImeAwareInput(search);
  useEffect(() => {
    if (searchInput.committedValue !== search) {
      setSearch(searchInput.committedValue);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput.committedValue]);

  const [modelFilter, setModelFilter] = useQueryState("model", {
    default: "",
    parser: z.string(),
    debounceMs: 0,
  });
  const [nextCursor, setNextCursor] = useState<string | undefined>();
  const [hasMore, setHasMore] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);

  const [viewMode, setViewMode] = useState<"conversation" | "events">("conversation");
  const [messages, setMessages] = useState<SessionMessage[]>([]);
  const [messagesLoading, setMessagesLoading] = useState(false);

  const confirm = useConfirm();

  useEffect(() => {
    listAgentSessions({ limit: PAGE_LIMIT })
      .then((page) => {
        setSessions(page.items);
        setNextCursor(page.next_cursor);
        setHasMore(page.items.length === PAGE_LIMIT);
      })
      .catch(() => toast.error(t("loadError")))
      .finally(() => setLoading(false));
  }, [t]);

  const handleLoadMore = async () => {
    if (!hasMore || loadingMore) return;
    setLoadingMore(true);
    try {
      const page = await listAgentSessions({ limit: PAGE_LIMIT, cursor: nextCursor });
      setSessions((prev) => [...prev, ...page.items]);
      setNextCursor(page.next_cursor);
      setHasMore(page.items.length === PAGE_LIMIT);
    } catch {
      toast.error(t("loadMoreError"));
    } finally {
      setLoadingMore(false);
    }
  };

  const stats = useMemo(() => {
    const total = sessions.length;
    const completed = sessions.filter((s) => s.completed_at).length;
    const sevenDaysAgo = Date.now() - 7 * 24 * 60 * 60 * 1000;
    const last7Days = sessions.filter(
      (s) => new Date(s.created_at).getTime() >= sevenDaysAgo,
    ).length;
    const modelsUsed = new Set(sessions.map((s) => s.model_id).filter(Boolean)).size;
    return { total, completed, last7Days, modelsUsed };
  }, [sessions]);

  const uniqueModels = useMemo(
    () => [...new Set(sessions.map((s) => s.model_id).filter(Boolean))].sort(),
    [sessions],
  );

  const filtered = sessions.filter((s) => {
    if (search && !s.user_message.toLowerCase().includes(search.toLowerCase())) return false;
    if (modelFilter && s.model_id !== modelFilter) return false;
    return true;
  });

  useEffect(() => {
    if (!selectedId) {
      setEvents([]);
      setMessages([]);
      return;
    }
    setEventsLoading(true);
    listAgentEvents(selectedId)
      .then(setEvents)
      .catch(() => toast.error(t("eventsError")))
      .finally(() => setEventsLoading(false));
  }, [selectedId, t]);

  useEffect(() => {
    if (viewMode === "conversation" && selectedId) {
      setMessagesLoading(true);
      fetchSessionMessages(selectedId)
        .then((res) => setMessages(res.messages))
        .catch(() => toast.error(t("messagesError")))
        .finally(() => setMessagesLoading(false));
    }
  }, [viewMode, selectedId, t]);

  const handleDelete = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    const ok = await confirm({
      title: t("deleteConfirmTitle"),
      description: t("deleteConfirmDescription"),
      confirmLabel: t("deleteConfirmLabel"),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteSession(sessionId);
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
      if (selectedId === sessionId) setSelectedId(null);
      toast.success(t("deletedToast"));
    } catch {
      toast.error(t("deleteError"));
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Spinner size="lg" />
      </div>
    );
  }

  const selected = sessions.find((s) => s.id === selectedId);

  return (
    <div>
      <h1 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">
        {t("title")}
      </h1>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("description")}
      </p>

      <div className="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard label={t("stats.total")} value={stats.total} />
        <StatCard label={t("stats.completed")} value={stats.completed} />
        <StatCard label={t("stats.last7Days")} value={stats.last7Days} />
        <StatCard label={t("stats.modelsUsed")} value={stats.modelsUsed} />
      </div>

      <div className="mt-4 mb-4 flex items-end gap-3">
        <FormInput
          type="search"
          placeholder={t("searchPlaceholder")}
          value={searchInput.value}
          onChange={searchInput.bind.onChange}
          onCompositionStart={searchInput.bind.onCompositionStart}
          onCompositionEnd={searchInput.bind.onCompositionEnd}
          className="max-w-xs"
        />
        <SettingsSelect
          label={t("modelFilterLabel")}
          hideLabel
          value={modelFilter}
          onChange={(e) => setModelFilter(e.target.value)}
          className="max-w-[200px]"
        >
          <option value="">{t("allModels")}</option>
          {uniqueModels.map((m) => (
            <option key={m} value={m}>
              {m.split("/").pop()}
            </option>
          ))}
        </SettingsSelect>
      </div>

      <div className="mt-6 space-y-2">
        {filtered.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {sessions.length === 0 ? t("empty") : t("emptyFiltered")}
          </p>
        ) : (
          filtered.map((s) => (
            <div
              key={s.id}
              className={`group relative rounded-md border px-4 py-3 text-left transition-colors ${
                s.id === selectedId
                  ? "border-emerald-500 bg-emerald-50/50 dark:bg-emerald-950/20"
                  : "border-zinc-200 hover:border-zinc-300 dark:border-zinc-800 dark:hover:border-zinc-700"
              }`}
            >
              <button
                onClick={() => setSelectedId(s.id === selectedId ? null : s.id)}
                className="w-full text-left"
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-zinc-700 dark:text-zinc-300 truncate max-w-md">
                    {s.user_message}
                  </span>
                  <span className="shrink-0 text-[10px] text-muted-foreground">
                    {new Date(s.created_at).toLocaleString()}
                  </span>
                </div>
                <div className="mt-1 flex items-center gap-3 text-[10px] text-muted-foreground">
                  <span>{s.model_id.split("/").pop()}</span>
                  {s.completed_at ? (
                    <span className="text-emerald-500">{t("completed")}</span>
                  ) : (
                    <span className="text-amber-500">{t("incomplete")}</span>
                  )}
                </div>
              </button>
              <button
                onClick={(e) => handleDelete(e, s.id)}
                className="absolute right-2 top-2 hidden rounded px-1.5 py-0.5 text-[10px] font-medium text-red-600 transition-colors hover:bg-red-50 group-hover:inline-block dark:text-red-400 dark:hover:bg-red-950/30"
              >
                {t("delete")}
              </button>
            </div>
          ))
        )}
      </div>

      {hasMore && sessions.length > 0 && (
        <div className="mt-4 flex justify-center">
          <Button variant="outline" size="sm" onClick={handleLoadMore} disabled={loadingMore}>
            {loadingMore
              ? t("loading")
              : t("loadMore", { count: sessions.length })}
          </Button>
        </div>
      )}

      {selected && (
        <div className="mt-6">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
              {t("detailHeading")}
            </h2>
            <div className="flex rounded-md border border-zinc-200 text-[10px] font-medium dark:border-zinc-700">
              <button
                onClick={() => setViewMode("conversation")}
                className={`px-3 py-1 transition-colors ${
                  viewMode === "conversation"
                    ? "bg-zinc-100 text-zinc-800 dark:bg-zinc-800 dark:text-zinc-200"
                    : "text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300"
                }`}
              >
                {t("viewConversation")}
              </button>
              <button
                onClick={() => setViewMode("events")}
                className={`px-3 py-1 transition-colors ${
                  viewMode === "events"
                    ? "bg-zinc-100 text-zinc-800 dark:bg-zinc-800 dark:text-zinc-200"
                    : "text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300"
                }`}
              >
                {t("viewEvents")}
              </button>
            </div>
          </div>
          <div className="mt-1 text-[10px] text-muted-foreground font-mono">
            {t("hashLine", {
              prompt: selected.prompt_hash.slice(0, 16),
              tool: selected.tool_schema_hash.slice(0, 16),
            })}
          </div>

          {viewMode === "conversation" ? (
            <ConversationView messages={messages} loading={messagesLoading} />
          ) : (
            <EventsView events={events} loading={eventsLoading} />
          )}
        </div>
      )}
    </div>
  );
}

function ConversationView({
  messages,
  loading,
}: {
  messages: SessionMessage[];
  loading: boolean;
}) {
  const t = useTranslations("settings.sessions");
  if (loading) {
    return (
      <div className="flex items-center justify-center py-4">
        <Spinner size="sm" />
      </div>
    );
  }

  if (messages.length === 0) {
    return <p className="mt-3 text-xs text-muted-foreground">{t("noMessages")}</p>;
  }

  return (
    <div className="mt-3 space-y-3">
      {messages.map((msg, i) => (
        <div key={i}>
          {msg.role === "user" ? (
            <div className="flex justify-end">
              <div className="max-w-[80%] rounded-lg bg-zinc-100 px-3 py-2 dark:bg-zinc-800">
                <p className="mb-1 text-[10px] font-semibold text-muted-foreground">
                  {t("roleUser")}
                </p>
                <p className="whitespace-pre-wrap text-xs text-zinc-700 dark:text-zinc-300">
                  {msg.content}
                </p>
              </div>
            </div>
          ) : (
            <div className="flex justify-start">
              <div className="max-w-[80%] space-y-2">
                <div className="rounded-lg border border-zinc-200 bg-white px-3 py-2 dark:border-zinc-800 dark:bg-zinc-900">
                  <p className="mb-1 text-[10px] font-semibold text-muted-foreground">
                    {t("roleAssistant")}
                  </p>
                  {msg.content && (
                    <p className="whitespace-pre-wrap text-xs text-zinc-700 dark:text-zinc-300">
                      {msg.content}
                    </p>
                  )}
                </div>
                {msg.tool_calls?.map((tc, j) => (
                  <div
                    key={j}
                    className="rounded-md border border-amber-200 bg-amber-50/50 px-3 py-1.5 dark:border-amber-800/50 dark:bg-amber-950/20"
                  >
                    <div className="flex items-center gap-2 text-[10px]">
                      <span className="font-semibold text-amber-700 dark:text-amber-400">
                        {tc.name}
                      </span>
                      {tc.duration_ms != null && (
                        <span className="text-muted-foreground">{tc.duration_ms}ms</span>
                      )}
                      <span
                        className={
                          tc.status === "error"
                            ? "text-red-500"
                            : tc.status === "review"
                              ? "text-blue-500"
                              : "text-emerald-500"
                        }
                      >
                        {tc.status}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function EventsView({
  events,
  loading,
}: {
  events: AgentEvent[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-4">
        <Spinner size="sm" />
      </div>
    );
  }

  return (
    <div className="mt-3 space-y-1">
      {events.map((e) => (
        <div
          key={e.id}
          className="flex items-start gap-3 rounded-md border border-zinc-100 px-3 py-2 dark:border-zinc-800"
        >
          <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] font-mono text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground">
            #{e.sequence}
          </span>
          <EventBadge type={e.event_type} />
          <span className="flex-1 truncate text-xs text-zinc-600 dark:text-muted-foreground font-mono">
            {JSON.stringify(e.payload).slice(0, 120)}
          </span>
          <span className="shrink-0 text-[10px] text-muted-foreground">
            {new Date(e.created_at).toLocaleTimeString()}
          </span>
        </div>
      ))}
    </div>
  );
}

function EventBadge({ type }: { type: string }) {
  const colors: Record<string, string> = {
    text: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
    tool_start: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
    tool_complete: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    complete: "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
    usage: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground",
    error: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  };

  return (
    <span
      className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${
        colors[type] ?? "bg-zinc-100 text-zinc-600"
      }`}
    >
      {type}
    </span>
  );
}

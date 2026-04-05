"use client";

import { useEffect, useMemo, useState } from "react";
import { Spinner } from "@/components/ui/spinner";
import { FormInput, SettingsSelect } from "@/components/ui/form-input";
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

// ---------------------------------------------------------------------------
// StatCard
// ---------------------------------------------------------------------------

function StatCard({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="rounded-lg border border-zinc-200 bg-white px-4 py-3 dark:border-zinc-800 dark:bg-zinc-900">
      <p className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-zinc-900 dark:text-zinc-100">{value}</p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// SessionsPage
// ---------------------------------------------------------------------------

export default function SessionsPage() {
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [eventsLoading, setEventsLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [modelFilter, setModelFilter] = useState("");
  const [nextCursor, setNextCursor] = useState<string | undefined>();
  const [hasMore, setHasMore] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);

  // Conversation replay state
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
      .catch(() => toast.error("Failed to load sessions"))
      .finally(() => setLoading(false));
  }, []);

  const handleLoadMore = async () => {
    if (!hasMore || loadingMore) return;
    setLoadingMore(true);
    try {
      const page = await listAgentSessions({ limit: PAGE_LIMIT, cursor: nextCursor });
      setSessions((prev) => [...prev, ...page.items]);
      setNextCursor(page.next_cursor);
      setHasMore(page.items.length === PAGE_LIMIT);
    } catch {
      toast.error("Failed to load more sessions");
    } finally {
      setLoadingMore(false);
    }
  };

  // --- Summary stats ---
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

  // --- Model filter options ---
  const uniqueModels = useMemo(
    () => [...new Set(sessions.map((s) => s.model_id).filter(Boolean))].sort(),
    [sessions],
  );

  // --- Filtered list ---
  const filtered = sessions.filter((s) => {
    if (search && !s.user_message.toLowerCase().includes(search.toLowerCase())) return false;
    if (modelFilter && s.model_id !== modelFilter) return false;
    return true;
  });

  // --- Load events when session selected ---
  useEffect(() => {
    if (!selectedId) {
      setEvents([]);
      setMessages([]);
      return;
    }
    setEventsLoading(true);
    listAgentEvents(selectedId)
      .then(setEvents)
      .catch(() => toast.error("Failed to load events"))
      .finally(() => setEventsLoading(false));
  }, [selectedId]);

  // --- Fetch messages when switching to conversation mode ---
  useEffect(() => {
    if (viewMode === "conversation" && selectedId) {
      setMessagesLoading(true);
      fetchSessionMessages(selectedId)
        .then((res) => setMessages(res.messages))
        .catch(() => toast.error("Failed to load messages"))
        .finally(() => setMessagesLoading(false));
    }
  }, [viewMode, selectedId]);

  // --- Delete session ---
  const handleDelete = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    const ok = await confirm({
      title: "Delete session",
      description: "This will permanently delete the session and all its events. This cannot be undone.",
      confirmLabel: "Delete",
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteSession(sessionId);
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
      if (selectedId === sessionId) setSelectedId(null);
      toast.success("Session deleted");
    } catch {
      toast.error("Failed to delete session");
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
        Agent Sessions
      </h1>
      <p className="mt-1 text-sm text-zinc-500">
        Audit trail of agent executions with event replay.
      </p>

      {/* Summary stats */}
      <div className="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard label="Total Sessions" value={stats.total} />
        <StatCard label="Completed" value={stats.completed} />
        <StatCard label="Last 7 Days" value={stats.last7Days} />
        <StatCard label="Models Used" value={stats.modelsUsed} />
      </div>

      {/* Search + Model filter */}
      <div className="mt-4 mb-4 flex items-end gap-3">
        <FormInput
          placeholder="Search by message..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="max-w-xs"
        />
        <SettingsSelect
          value={modelFilter}
          onChange={(e) => setModelFilter(e.target.value)}
          className="max-w-[200px]"
        >
          <option value="">All models</option>
          {uniqueModels.map((m) => (
            <option key={m} value={m}>
              {m.split("/").pop()}
            </option>
          ))}
        </SettingsSelect>
      </div>

      {/* Session list */}
      <div className="mt-6 space-y-2">
        {filtered.length === 0 ? (
          <p className="text-sm text-zinc-400">
            {sessions.length === 0 ? "No sessions recorded yet." : "No matching sessions."}
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
                  <span className="shrink-0 text-[10px] text-zinc-400">
                    {new Date(s.created_at).toLocaleString()}
                  </span>
                </div>
                <div className="mt-1 flex items-center gap-3 text-[10px] text-zinc-400">
                  <span>{s.model_id.split("/").pop()}</span>
                  {s.completed_at ? (
                    <span className="text-emerald-500">completed</span>
                  ) : (
                    <span className="text-amber-500">incomplete</span>
                  )}
                </div>
              </button>
              <button
                onClick={(e) => handleDelete(e, s.id)}
                className="absolute right-2 top-2 hidden rounded px-1.5 py-0.5 text-[10px] font-medium text-red-600 transition-colors hover:bg-red-50 group-hover:inline-block dark:text-red-400 dark:hover:bg-red-950/30"
              >
                Delete
              </button>
            </div>
          ))
        )}
      </div>

      {hasMore && sessions.length > 0 && (
        <div className="mt-4 flex justify-center">
          <Button variant="outline" size="sm" onClick={handleLoadMore} disabled={loadingMore}>
            {loadingMore ? "Loading..." : `Load more (showing ${sessions.length})`}
          </Button>
        </div>
      )}

      {/* Detail panel */}
      {selected && (
        <div className="mt-6">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
              Session Detail
            </h2>
            {/* Tab toggle */}
            <div className="flex rounded-md border border-zinc-200 text-[10px] font-medium dark:border-zinc-700">
              <button
                onClick={() => setViewMode("conversation")}
                className={`px-3 py-1 transition-colors ${
                  viewMode === "conversation"
                    ? "bg-zinc-100 text-zinc-800 dark:bg-zinc-800 dark:text-zinc-200"
                    : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
                }`}
              >
                Conversation
              </button>
              <button
                onClick={() => setViewMode("events")}
                className={`px-3 py-1 transition-colors ${
                  viewMode === "events"
                    ? "bg-zinc-100 text-zinc-800 dark:bg-zinc-800 dark:text-zinc-200"
                    : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
                }`}
              >
                Events
              </button>
            </div>
          </div>
          <div className="mt-1 text-[10px] text-zinc-400 font-mono">
            prompt_hash: {selected.prompt_hash.slice(0, 16)}... ·
            tool_hash: {selected.tool_schema_hash.slice(0, 16)}...
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

// ---------------------------------------------------------------------------
// ConversationView — chat-bubble replay
// ---------------------------------------------------------------------------

function ConversationView({
  messages,
  loading,
}: {
  messages: SessionMessage[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-4">
        <Spinner size="sm" />
      </div>
    );
  }

  if (messages.length === 0) {
    return <p className="mt-3 text-xs text-zinc-400">No messages.</p>;
  }

  return (
    <div className="mt-3 space-y-3">
      {messages.map((msg, i) => (
        <div key={i}>
          {msg.role === "user" ? (
            <div className="flex justify-end">
              <div className="max-w-[80%] rounded-lg bg-zinc-100 px-3 py-2 dark:bg-zinc-800">
                <p className="mb-1 text-[10px] font-semibold text-zinc-500">User</p>
                <p className="whitespace-pre-wrap text-xs text-zinc-700 dark:text-zinc-300">
                  {msg.content}
                </p>
              </div>
            </div>
          ) : (
            <div className="flex justify-start">
              <div className="max-w-[80%] space-y-2">
                <div className="rounded-lg border border-zinc-200 bg-white px-3 py-2 dark:border-zinc-800 dark:bg-zinc-900">
                  <p className="mb-1 text-[10px] font-semibold text-zinc-500">Assistant</p>
                  {msg.content && (
                    <p className="whitespace-pre-wrap text-xs text-zinc-700 dark:text-zinc-300">
                      {msg.content}
                    </p>
                  )}
                </div>
                {/* Tool calls */}
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
                        <span className="text-zinc-400">{tc.duration_ms}ms</span>
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

// ---------------------------------------------------------------------------
// EventsView — raw event timeline (original)
// ---------------------------------------------------------------------------

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
          <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] font-mono text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">
            #{e.sequence}
          </span>
          <EventBadge type={e.event_type} />
          <span className="flex-1 truncate text-xs text-zinc-600 dark:text-zinc-400 font-mono">
            {JSON.stringify(e.payload).slice(0, 120)}
          </span>
          <span className="shrink-0 text-[10px] text-zinc-400">
            {new Date(e.created_at).toLocaleTimeString()}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// EventBadge
// ---------------------------------------------------------------------------

function EventBadge({ type }: { type: string }) {
  const colors: Record<string, string> = {
    text: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
    tool_start: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
    tool_complete: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    complete: "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
    usage: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
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

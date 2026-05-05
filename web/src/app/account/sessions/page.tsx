"use client";

import { useEffect, useMemo, useState } from "react";
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { z } from "zod";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { Clock } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { FormInput, SettingsSelect } from "@/components/ui/form-input";
import { useQueryState } from "@/hooks/use-query-state";
import { useImeAwareInput } from "@/hooks/use-ime-aware-input";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { useConfirm } from "@/components/providers/confirm-provider";
import { useFormatters } from "@/hooks/use-formatters";
import type { AgentSession, AgentEvent, SessionMessage } from "@/types/api";
import {
  listAgentSessions,
  listAgentEvents,
  fetchSessionMessages,
  deleteSession,
} from "@/lib/api";

const PAGE_LIMIT = 50;

const sessionsKeys = {
  all: ["sessions"] as const,
  list: () => [...sessionsKeys.all, "list"] as const,
  events: (id: string) => [...sessionsKeys.all, "events", id] as const,
  messages: (id: string) => [...sessionsKeys.all, "messages", id] as const,
};

function StatCard({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="rounded-lg border border-divider bg-surface-base px-4 py-3">
      <p className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-foreground-strong">{value}</p>
    </div>
  );
}

export default function SessionsPage() {
  const t = useTranslations("account.sessions");
  const tCommon = useTranslations("common");
  const qc = useQueryClient();
  const confirm = useConfirm();
  const fmt = useFormatters();

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useQueryState("q", {
    default: "",
    parser: z.string(),
  });
  const searchInput = useImeAwareInput(search);
  useEffect(() => {
    if (searchInput.committedValue !== search) {
      setSearch(searchInput.committedValue);
    }
  }, [searchInput.committedValue, setSearch, search]);

  const [modelFilter, setModelFilter] = useQueryState("model", {
    default: "",
    parser: z.string(),
    debounceMs: 0,
  });

  const [viewMode, setViewMode] = useState<"conversation" | "events">("conversation");

  const sessionsQuery = useInfiniteQuery({
    queryKey: sessionsKeys.list(),
    queryFn: ({ pageParam }) =>
      listAgentSessions({
        limit: PAGE_LIMIT,
        cursor: pageParam as string | undefined,
      }),
    initialPageParam: undefined as string | undefined,
    // Defensive against an undefined / malformed page payload — a
    // network glitch or backend shape regression should land in the
    // `isError` branch rather than crash the pagination machinery.
    getNextPageParam: (last) =>
      last?.items?.length === PAGE_LIMIT ? last.next_cursor : undefined,
  });

  const sessions: AgentSession[] = useMemo(
    () => sessionsQuery.data?.pages.flatMap((p) => p?.items ?? []) ?? [],
    [sessionsQuery.data],
  );

  const eventsQuery = useQuery({
    queryKey: selectedId ? sessionsKeys.events(selectedId) : sessionsKeys.all,
    queryFn: () => listAgentEvents(selectedId!),
    enabled: !!selectedId,
  });
  const events: AgentEvent[] = eventsQuery.data ?? [];

  const messagesQuery = useQuery({
    queryKey: selectedId ? sessionsKeys.messages(selectedId) : sessionsKeys.all,
    queryFn: () => fetchSessionMessages(selectedId!),
    enabled: !!selectedId && viewMode === "conversation",
  });
  const messages: SessionMessage[] = messagesQuery.data?.messages ?? [];

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteSession(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: sessionsKeys.list() });
      if (selectedId === id) setSelectedId(null);
      toast.success(t("toast.deleted"));
    },
    onError: () => toast.error(t("toast.deleteFailed")),
  });

  // Capture the "now" anchor once at mount via lazy state init —
  // keeps the 7-day window stable across re-renders so a sub-second
  // re-render doesn't reshuffle the bucket boundaries, and lifts
  // the `Date.now()` call out of the render path that React 19's
  // purity check guards.
  const [mountedAt] = useState(() => Date.now());
  const stats = useMemo(() => {
    const total = sessions.length;
    const completed = sessions.filter((s) => s.completed_at).length;
    const sevenDaysAgo = mountedAt - 7 * 24 * 60 * 60 * 1000;
    const last7Days = sessions.filter(
      (s) => new Date(s.created_at).getTime() >= sevenDaysAgo,
    ).length;
    const modelsUsed = new Set(sessions.map((s) => s.model_id).filter(Boolean)).size;
    return { total, completed, last7Days, modelsUsed };
  }, [sessions, mountedAt]);

  const uniqueModels = useMemo(
    () => [...new Set(sessions.map((s) => s.model_id).filter(Boolean))].sort(),
    [sessions],
  );

  const filtered = sessions.filter((s) => {
    if (search && !s.user_message.toLowerCase().includes(search.toLowerCase())) return false;
    if (modelFilter && s.model_id !== modelFilter) return false;
    return true;
  });

  const handleDelete = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    const ok = await confirm({
      title: t("deleteConfirmTitle"),
      description: t("deleteConfirmDescription"),
      confirmLabel: t("deleteConfirmLabel"),
      variant: "danger",
    });
    if (!ok) return;
    deleteMutation.mutate(sessionId);
  };

  const selected = sessions.find((s) => s.id === selectedId);

  const pageState: PageState = sessionsQuery.isLoading
    ? { kind: "loading" }
    : sessionsQuery.isError
      ? { kind: "error", onRetry: () => void sessionsQuery.refetch() }
      : { kind: "data" };

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      <PageStateView
        state={pageState}
        skeleton={<SkeletonList count={5} />}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
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
          <EmptyState
            icon={Clock}
            title={sessions.length === 0 ? t("empty") : t("emptyFiltered")}
          />
        ) : (
          filtered.map((s) => (
            <div
              key={s.id}
              className={`group relative rounded-md border px-4 py-3 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                s.id === selectedId
                  ? "border-brand-foreground bg-brand-surface"
                  : "border-divider hover:border-divider"
              }`}
            >
              <button
                type="button"
                onClick={() => setSelectedId(s.id === selectedId ? null : s.id)}
                className="w-full text-start"
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-foreground truncate max-w-md">
                    {s.user_message}
                  </span>
                  <span className="shrink-0 text-2xs text-foreground-muted">
                    {fmt.date(s.created_at)}
                  </span>
                </div>
                <div className="mt-1 flex items-center gap-3 text-2xs text-foreground-muted">
                  <span>{s.model_id.split("/").pop()}</span>
                  {s.completed_at ? (
                    <span className="text-brand-foreground">{t("completed")}</span>
                  ) : (
                    <span className="text-warning-foreground">{t("incomplete")}</span>
                  )}
                </div>
              </button>
              <button
                type="button"
                onClick={(e) => handleDelete(e, s.id)}
                className="absolute end-2 top-2 hidden rounded px-1.5 py-0.5 text-2xs font-medium text-danger-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-danger-surface group-hover:inline-block"
              >
                {tCommon("delete")}
              </button>
            </div>
          ))
        )}
      </div>

      {sessionsQuery.hasNextPage && sessions.length > 0 && (
        <div className="mt-4 flex justify-center">
          <Button
            variant="outline"
            size="sm"
            onClick={() => sessionsQuery.fetchNextPage()}
            disabled={sessionsQuery.isFetchingNextPage}
          >
            {sessionsQuery.isFetchingNextPage
              ? tCommon("loading")
              : t("loadMore", { count: sessions.length })}
          </Button>
        </div>
      )}

      {selected && (
        <div className="mt-6">
          <div className="flex items-center justify-between">
            <Heading level={2} size={6}>
              {t("detailHeading")}
            </Heading>
            <div className="flex rounded-md border border-divider text-2xs font-medium">
              <button
                type="button"
                onClick={() => setViewMode("conversation")}
                className={`px-3 py-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                  viewMode === "conversation"
                    ? "bg-surface-inset text-foreground-strong"
                    : "text-foreground-muted hover:text-foreground"
                }`}
              >
                {t("viewConversation")}
              </button>
              <button
                type="button"
                onClick={() => setViewMode("events")}
                className={`px-3 py-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                  viewMode === "events"
                    ? "bg-surface-inset text-foreground-strong"
                    : "text-foreground-muted hover:text-foreground"
                }`}
              >
                {t("viewEvents")}
              </button>
            </div>
          </div>
          <div className="mt-1 text-2xs text-foreground-muted font-mono">
            {t("hashLine", {
              prompt: selected.prompt_hash.slice(0, 16),
              tool: selected.tool_schema_hash.slice(0, 16),
            })}
          </div>

          {viewMode === "conversation" ? (
            <ConversationView messages={messages} loading={messagesQuery.isLoading} />
          ) : (
            <EventsView events={events} loading={eventsQuery.isLoading} />
          )}
        </div>
      )}
      </PageStateView>
    </SettingsPageShell>
  );
}

function ConversationView({
  messages,
  loading,
}: {
  messages: SessionMessage[];
  loading: boolean;
}) {
  const t = useTranslations("account.sessions");
  if (loading) {
    return (
      <div className="flex items-center justify-center py-4">
        <Spinner size="sm" />
      </div>
    );
  }

  if (messages.length === 0) {
    return <p className="mt-3 text-xs text-foreground-muted">{t("noMessages")}</p>;
  }

  return (
    <div className="mt-3 space-y-3">
      {messages.map((msg, i) => (
        <div key={i}>
          {msg.role === "user" ? (
            <div className="flex justify-end">
              <div className="max-w-[80%] rounded-lg bg-surface-inset px-3 py-2">
                <p className="mb-1 text-2xs font-semibold text-foreground-muted">
                  {t("roleUser")}
                </p>
                <p className="whitespace-pre-wrap text-xs text-foreground">
                  {msg.content}
                </p>
              </div>
            </div>
          ) : (
            <div className="flex justify-start">
              <div className="max-w-[80%] space-y-2">
                <div className="rounded-lg border border-divider bg-surface-base px-3 py-2">
                  <p className="mb-1 text-2xs font-semibold text-foreground-muted">
                    {t("roleAssistant")}
                  </p>
                  {msg.content && (
                    <p className="whitespace-pre-wrap text-xs text-foreground">
                      {msg.content}
                    </p>
                  )}
                </div>
                {msg.tool_calls?.map((tc, j) => (
                  <div
                    key={j}
                    className="rounded-md border border-warning-border bg-warning-surface px-3 py-1.5"
                  >
                    <div className="flex items-center gap-2 text-2xs">
                      <span className="font-semibold text-warning-foreground">
                        {tc.name}
                      </span>
                      {tc.duration_ms != null && (
                        <span className="text-foreground-muted">{tc.duration_ms}ms</span>
                      )}
                      <span
                        className={
                          tc.status === "error"
                            ? "text-danger-foreground"
                            : tc.status === "review"
                              ? "text-info-foreground"
                              : "text-brand-foreground"
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
          className="flex items-start gap-3 rounded-md border border-divider-soft px-3 py-2"
        >
          <span className="shrink-0 rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-mono text-foreground">
            #{e.sequence}
          </span>
          <EventBadge type={e.event_type} />
          <span className="flex-1 truncate text-xs text-foreground font-mono">
            {JSON.stringify(e.payload).slice(0, 120)}
          </span>
          <span className="shrink-0 text-2xs text-foreground-muted">
            {new Date(e.created_at).toLocaleTimeString()}
          </span>
        </div>
      ))}
    </div>
  );
}

function EventBadge({ type }: { type: string }) {
  const colors: Record<string, string> = {
    text: "bg-info-surface text-info-foreground",
    tool_start: "bg-warning-surface text-warning-foreground",
    tool_complete: "bg-success-surface text-success-foreground",
    complete: "bg-concept-surface text-concept-foreground",
    usage: "bg-surface-inset text-foreground",
    error: "bg-danger-surface text-danger-foreground",
  };

  return (
    <span
      className={`shrink-0 rounded px-1.5 py-0.5 text-2xs font-medium ${
        colors[type] ?? "bg-surface-inset text-foreground"
      }`}
    >
      {type}
    </span>
  );
}

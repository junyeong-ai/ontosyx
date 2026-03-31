"use client";

import { useEffect, useState } from "react";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import type { AgentSession, AgentEvent } from "@/types/api";
import { listAgentSessions, listAgentEvents } from "@/lib/api";

export default function SessionsPage() {
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [eventsLoading, setEventsLoading] = useState(false);

  useEffect(() => {
    listAgentSessions({ limit: 50 })
      .then((page) => setSessions(page.items))
      .catch(() => toast.error("Failed to load sessions"))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setEvents([]);
      return;
    }
    setEventsLoading(true);
    listAgentEvents(selectedId)
      .then(setEvents)
      .catch(() => toast.error("Failed to load events"))
      .finally(() => setEventsLoading(false));
  }, [selectedId]);

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

      <div className="mt-6 space-y-2">
        {sessions.length === 0 ? (
          <p className="text-sm text-zinc-400">No sessions recorded yet.</p>
        ) : (
          sessions.map((s) => (
            <button
              key={s.id}
              onClick={() => setSelectedId(s.id === selectedId ? null : s.id)}
              className={`w-full rounded-md border px-4 py-3 text-left transition-colors ${
                s.id === selectedId
                  ? "border-emerald-500 bg-emerald-50/50 dark:bg-emerald-950/20"
                  : "border-zinc-200 hover:border-zinc-300 dark:border-zinc-800 dark:hover:border-zinc-700"
              }`}
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
          ))
        )}
      </div>

      {/* Event timeline */}
      {selected && (
        <div className="mt-6">
          <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
            Event Timeline
          </h2>
          <div className="mt-1 text-[10px] text-zinc-400 font-mono">
            prompt_hash: {selected.prompt_hash.slice(0, 16)}... ·
            tool_hash: {selected.tool_schema_hash.slice(0, 16)}...
          </div>

          {eventsLoading ? (
            <div className="flex items-center justify-center py-4">
              <Spinner size="sm" />
            </div>
          ) : (
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
          )}
        </div>
      )}
    </div>
  );
}

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

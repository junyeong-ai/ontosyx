"use client";

import { useEffect, useState } from "react";
import { useAppStore, type ToolCall, type ChatMessage } from "@/lib/store";
import type { AgentSession as AgentSessionType } from "@/types/api";
import { listAgentSessions, fetchSessionMessages } from "@/lib/api";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatRelativeDate(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return "just now";
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays}d ago`;
  return date.toLocaleDateString();
}

// ---------------------------------------------------------------------------
// SessionBar — compact session switcher at top of Chat area
// ---------------------------------------------------------------------------

export function SessionBar() {
  const sessionId = useAppStore((s) => s.sessionId);
  const clearMessages = useAppStore((s) => s.clearMessages);
  const [sessions, setSessions] = useState<AgentSessionType[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    listAgentSessions({ limit: 20 })
      .then((page) => setSessions(page.items))
      .catch(() => { /* non-critical: session list fetch */ });
  }, []);

  return (
    <div className="flex h-8 shrink-0 items-center gap-2 border-b border-zinc-200 px-3 dark:border-zinc-800">
      <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
        Session
      </span>
      <span className="truncate text-xs text-zinc-600 dark:text-zinc-400 max-w-[140px]">
        {sessionId ? sessionId.slice(0, 8) + "..." : "New"}
      </span>

      <div className="flex-1" />

      <button
        onClick={() => setOpen(!open)}
        className="text-[10px] text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
      >
        {sessions.length} past
      </button>

      <button
        onClick={clearMessages}
        className="rounded px-1.5 py-0.5 text-[10px] font-medium text-emerald-600 hover:bg-emerald-50 dark:hover:bg-emerald-950"
      >
        New
      </button>

      {/* Session dropdown */}
      {open && sessions.length > 0 && (
        <div className="absolute left-0 top-8 z-20 w-80 rounded-lg border border-zinc-200 bg-white shadow-lg dark:border-zinc-700 dark:bg-zinc-900">
          <div className="max-h-60 overflow-auto p-1">
            {sessions.map((s) => (
              <button
                key={s.id}
                onClick={async () => {
                  try {
                    const { messages } = await fetchSessionMessages(s.id);
                    const chatMessages: ChatMessage[] = messages.map((m, i) => ({
                      id: `restored-${i}`,
                      role: m.role,
                      content: m.content,
                      thinking: m.thinking,
                      toolCalls: m.tool_calls?.map((tc) => ({
                        id: tc.id,
                        name: tc.name,
                        input: tc.input,
                        output: tc.output,
                        status: tc.status as ToolCall["status"],
                        durationMs: tc.duration_ms,
                      })),
                    }));
                    useAppStore.getState().restoreMessages(chatMessages);
                    useAppStore.getState().setSessionId(s.id);
                  } catch {
                    // silent fail
                  }
                  setOpen(false);
                }}
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-xs hover:bg-zinc-50 dark:hover:bg-zinc-800"
              >
                <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${s.completed_at ? 'bg-emerald-500' : 'bg-amber-400'}`} />
                <span className="flex-1 truncate text-zinc-700 dark:text-zinc-300">
                  {s.user_message.slice(0, 60)}
                </span>
                <span className="shrink-0 text-[10px] text-zinc-400">
                  {formatRelativeDate(s.created_at)}
                </span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

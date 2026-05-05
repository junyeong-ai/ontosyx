"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore, type ToolCall, type ChatMessage } from "@/lib/store";
import type { AgentSession as AgentSessionType } from "@/types/api";
import { listAgentSessions, fetchSessionMessages } from "@/lib/api";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type RelativeFmt = (key: string, params?: Record<string, string | number>) => string;

function formatRelativeDate(dateStr: string, t: RelativeFmt): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return t("relative.justNow");
  if (diffMins < 60) return t("relative.minutesAgo", { n: diffMins });
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return t("relative.hoursAgo", { n: diffHours });
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays === 1) return t("relative.yesterday");
  if (diffDays < 7) return t("relative.daysAgo", { n: diffDays });
  return date.toLocaleDateString();
}

// ---------------------------------------------------------------------------
// SessionBar — compact session switcher at top of Chat area
// ---------------------------------------------------------------------------

export function SessionBar() {
  const t = useTranslations("workbench.analyze.sessionBar");
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
    <div className="flex h-8 shrink-0 items-center gap-2 border-b border-divider px-3">
      <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("label")}
      </span>
      <span className="truncate text-xs text-foreground max-w-[140px]">
        {sessionId ? sessionId.slice(0, 8) + "..." : t("newSession")}
      </span>

      <div className="flex-1" />

      <button type="button"
        onClick={() => setOpen(!open)}
        className="text-2xs text-foreground-muted hover:text-foreground-muted"
      >
        {t("pastCount", { count: sessions.length })}
      </button>

      <button type="button"
        onClick={clearMessages}
        className="rounded px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface"
      >
        {t("newSession")}
      </button>

      {/* Session dropdown */}
      {open && sessions.length > 0 && (
        <div className="absolute start-0 top-8 z-popover w-80 rounded-lg border border-divider bg-surface-base shadow-3">
          <div className="max-h-60 overflow-auto p-1">
            {sessions.map((s) => (
              <button type="button"
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
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-start text-xs hover:bg-surface-raised"
              >
                <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${s.completed_at ? 'bg-brand-solid' : 'bg-warning-foreground'}`} />
                <span className="flex-1 truncate text-foreground">
                  {s.user_message.slice(0, 60)}
                </span>
                <span className="shrink-0 text-2xs text-foreground-muted">
                  {formatRelativeDate(s.created_at, t)}
                </span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore, type ChatMessage, type ToolStep } from "@/lib/store";
import { useWorkspaceMode } from "@/hooks/use-workspace-mode";
import { chatStream, fetchSessionMessages, listAgentSessions, rawQuery, suggestInsights, type InsightHint } from "@/lib/api";
import type { AgentSession } from "@/types/api";
import { errorMessage } from "@/lib/error-messages";
import { toast } from "sonner";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { ChatInput } from "./chat-input";
import { MessageBubble } from "./message-bubble";
import { motion, AnimatePresence } from "motion/react";
import { HugeiconsIcon } from "@hugeicons/react";
import { ChatBotIcon } from "@hugeicons/core-free-icons";

/** Upsert a step in the steps array (replace by step name, or append). */
function upsertToolStep(steps: ToolStep[], update: ToolStep): ToolStep[] {
  const idx = steps.findIndex((s) => s.step === update.step);
  if (idx >= 0) {
    const copy = [...steps];
    copy[idx] = update;
    return copy;
  }
  return [...steps, update];
}

export function ChatPanel() {
  const t = useTranslations("workbench.chat.panel");
  const {
    messages,
    isLoading,
    ontology,
    activeProject,
    addMessage,
    updateMessage,
    setIsLoading,
    sessionId,
    setSessionId,
    ontologyId,
  } = useAppStore();
  const workspaceMode = useWorkspaceMode();

  const scrollRef = useRef<HTMLDivElement>(null);
  const userScrolledUpRef = useRef(false);
  const abortRef = useRef<AbortController | null>(null);
  const [suggestions, setSuggestions] = useState<InsightHint[]>([]);
  const [recentSessions, setRecentSessions] = useState<AgentSession[]>([]);

  // Cancel in-flight stream on unmount
  useEffect(() => {
    return () => { abortRef.current?.abort(); };
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const handleScroll = () => {
      const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
      userScrolledUpRef.current = distanceFromBottom > 80;
    };
    el.addEventListener("scroll", handleScroll);
    return () => el.removeEventListener("scroll", handleScroll);
  }, []);

  // Load insight suggestions when ontology is present and chat is empty.
  // Depend on the full `ontology` (not just `?.id`) so the lint can see the
  // same reference that `suggestInsights(ontology)` actually closes over.
  // The zustand `selectStateOntology` returns a stable reference between
  // structural changes, so this won't churn the effect on every render.
  useEffect(() => {
    if (!ontology || messages.length > 0) {
      setSuggestions([]);
      return;
    }
    let cancelled = false;
    suggestInsights(ontology)
      .then((result) => {
        if (!cancelled) setSuggestions(result);
      })
      .catch(() => {
        // Silent fail — suggestions are a nice-to-have
      });
    return () => {
      cancelled = true;
    };
  }, [ontology, messages.length]);

  // Load recent sessions when chat is empty (for resume UI)
  useEffect(() => {
    if (messages.length > 0) {
      setRecentSessions([]);
      return;
    }
    let cancelled = false;
    listAgentSessions({ limit: 5 })
      .then((page) => {
        if (!cancelled) setRecentSessions(page.items);
      })
      .catch(() => {
        // Silent — recent sessions are a nice-to-have
      });
    return () => {
      cancelled = true;
    };
  }, [messages.length]);

  const handleResumeSession = useCallback(
    async (session: AgentSession) => {
      try {
        const { messages: prev } = await fetchSessionMessages(session.id);
        const restored: ChatMessage[] = prev.map((m) => ({
          id: crypto.randomUUID(),
          role: m.role,
          content: m.content,
          thinking: m.thinking,
          toolCalls: m.tool_calls?.map((tc) => ({
            id: tc.id,
            name: tc.name,
            input: tc.input,
            output: tc.output,
            status: tc.status === "error" ? ("error" as const) : ("done" as const),
            durationMs: tc.duration_ms,
          })),
        }));
        useAppStore.getState().restoreMessages(restored);
        setSessionId(session.id);
        toast.success(t("sessionResumed"));
      } catch {
        toast.error(t("resumeFailed"));
      }
    },
    [setSessionId, t],
  );

  const lastMessageContent = messages[messages.length - 1]?.content;
  const lastMessageStreaming = messages[messages.length - 1]?.isStreaming;
  useEffect(() => {
    if (userScrolledUpRef.current) return;
    scrollRef.current?.scrollTo({
      top: scrollRef.current.scrollHeight,
      behavior: lastMessageStreaming ? "instant" : "smooth",
    });
  }, [messages.length, lastMessageContent, lastMessageStreaming]);

  const getState = useAppStore.getState;

  const handleSend = useCallback(
    async (text: string) => {
      if (!ontology) return;

      // Raw Cypher mode: ! prefix
      const isRaw = text.startsWith("!");
      const actualText = isRaw ? text.slice(1).trim() : text;
      if (!actualText) return;

      const userMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content: text,
      };
      addMessage(userMsg);
      setIsLoading(true);

      // Raw query mode — direct Cypher execution
      if (isRaw) {
        const assistantId = crypto.randomUUID();
        addMessage({ id: assistantId, role: "assistant", content: "", isStreaming: true });
        try {
          const result = await rawQuery({ query: actualText });
          updateMessage(assistantId, {
            content: t("rawRowsSummary", { count: result.rows.length }),
            toolCalls: [{
              id: "raw",
              name: "raw_cypher",
              input: actualText,
              output: JSON.stringify(result, null, 2),
              status: "done" as const,
            }],
            isStreaming: false,
          });
        } catch (err) {
          updateMessage(assistantId, {
            error: err instanceof Error ? err.message : String(err),
            isStreaming: false,
          });
        } finally {
          setIsLoading(false);
        }
        return;
      }

      const assistantId = crypto.randomUUID();
      const assistantMsg: ChatMessage = {
        id: assistantId,
        role: "assistant",
        content: "",
        isStreaming: true,
        toolCalls: [],
      };
      addMessage(assistantMsg);

      // Helper to get current assistant message from store
      const getAssistant = () =>
        getState().messages.find((m) => m.id === assistantId);

      // Cancel any previous stream and create new abort controller
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      try {
        const isDesignMode = workspaceMode === "design";
        await chatStream(
          {
            message: text,
            ontology,
            ontology_id: isDesignMode
              ? (activeProject?.ontology_id ?? undefined)
              : (ontologyId ?? undefined),
            project_id: isDesignMode ? activeProject?.id : undefined,
            project_revision: isDesignMode ? activeProject?.revision : undefined,
            session_id: sessionId ?? undefined,
            execution_mode: getState().executionMode,
            model_override: getState().modelOverride ?? undefined,
          },
          {
            onText(delta) {
              // Text between tool calls is intermediate reasoning
              // (e.g., "과거 보정 패턴을 확인했습니다", "데이터를 확보했습니다").
              // Route to thinking if ANY tool has been called — only text
              // arriving BEFORE the first tool call or AFTER the final
              // tool_complete (when isLoading becomes false via onComplete)
              // goes to content. Since onComplete sets isStreaming=false,
              // and the last text chunk arrives before onComplete, we use
              // "has any toolCalls" as the divider.
              const msg = getAssistant();
              const hasAnyToolCalls = msg?.toolCalls && msg.toolCalls.length > 0;
              const allToolsDone = hasAnyToolCalls && msg.toolCalls!.every((tc) => tc.status === "done" || tc.status === "error");

              if (hasAnyToolCalls && !allToolsDone) {
                // Tools in progress or between calls → thinking
                updateMessage(assistantId, {
                  thinking: (msg?.thinking ?? "") + delta,
                });
              } else {
                // No tools yet (pre-tool text) or all done (final analysis)
                updateMessage(assistantId, {
                  content: (msg?.content ?? "") + delta,
                });
              }
            },
            onThinking(content) {
              updateMessage(assistantId, {
                thinking: (getAssistant()?.thinking ?? "") + content,
              });
            },
            onToolStart(event) {
              updateMessage(assistantId, {
                toolCalls: [
                  ...(getAssistant()?.toolCalls ?? []),
                  { id: event.id, name: event.name, input: event.input, status: "running" },
                ],
              });
            },
            onToolProgress(event) {
              const toolCalls = getAssistant()?.toolCalls ?? [];
              updateMessage(assistantId, {
                toolCalls: toolCalls.map((tc) =>
                  tc.id === event.tool_call_id
                    ? {
                        ...tc,
                        steps: upsertToolStep(tc.steps ?? [], {
                          step: event.step,
                          status: event.status,
                          durationMs: event.duration_ms,
                          metadata: event.metadata,
                        }),
                      }
                    : tc,
                ),
              });
            },
            onToolComplete(event) {
              updateMessage(assistantId, {
                toolCalls: (getAssistant()?.toolCalls ?? []).map((tc) =>
                  tc.id === event.id
                    ? {
                        ...tc,
                        output: event.output,
                        status: event.is_error ? "error" : "done",
                        durationMs: event.duration_ms,
                      }
                    : tc,
                ),
              });
              // Auto-switch to Results tab when query_graph completes with data
              if (event.name === "query_graph" && !event.is_error) {
                const store = useAppStore.getState();
                store.setAnalyzeRightTab("results");
                store.setFocusResultId(event.id);
              }
            },
            onToolReview(event) {
              updateMessage(assistantId, {
                toolCalls: [
                  ...(getAssistant()?.toolCalls ?? []),
                  { id: event.id, name: event.name, input: event.input, status: "review" },
                ],
              });
            },
            onUsage(event) {
              getState().setTokenUsage({
                input: event.input_tokens,
                output: event.output_tokens,
              });
            },
            onSessionExpired(event) {
              // Session resume failed — clear stale session and restore
              // previous messages as read-only history context.
              setSessionId(null);
              toast.warning(t("toast.sessionExpired"));

              // Best-effort: fetch previous conversation and prepend as
              // read-only history so the user still sees past context.
              fetchSessionMessages(event.previous_session_id)
                .then(({ messages: prev }) => {
                  if (prev.length === 0) return;
                  const state = getState();
                  const restored: ChatMessage[] = prev.map((m) => ({
                    id: crypto.randomUUID(),
                    role: m.role,
                    content: m.content,
                    thinking: m.thinking,
                    toolCalls: m.tool_calls?.map((tc) => ({
                      id: tc.id,
                      name: tc.name,
                      input: tc.input,
                      output: tc.output,
                      status: tc.status === "error" ? "error" as const : "done" as const,
                      durationMs: tc.duration_ms,
                    })),
                  }));
                  // Prepend restored messages before the current user message
                  // which is already in the store.
                  state.restoreMessages([...restored, ...state.messages]);
                })
                .catch(() => {
                  // Silent — restoration is best-effort.
                });
            },
            onComplete(event) {
              updateMessage(assistantId, { isStreaming: false });
              if (event.session_id) {
                setSessionId(event.session_id);
              }
            },
            onError(error) {
              updateMessage(assistantId, {
                content: "",
                error: errorMessage(undefined, String(error)),
                isStreaming: false,
              });
            },
          },
          controller.signal,
        );
      } catch (err) {
        if (controller.signal.aborted) return; // Unmount or cancel — not an error
        updateMessage(assistantId, {
          content: "",
          error: t("toast.connectionFailed", { error: err instanceof Error ? err.message : String(err) }),
          isStreaming: false,
        });
      } finally {
        setIsLoading(false);
      }
    },
    [
      ontology,
      activeProject,
      addMessage,
      updateMessage,
      setIsLoading,
      getState,
      ontologyId,
      sessionId,
      setSessionId,
      workspaceMode,
      t,
    ],
  );

  const inputDisabled = isLoading || !ontology;
  const disabledReason = !ontology ? t("inputDisabledNoOntology") : undefined;

  return (
    <ErrorBoundary name="Chat">
    <div className="flex h-full flex-col bg-surface-raised">
      <div ref={scrollRef} role="log" aria-label={t("logAria")} aria-live="polite" tabIndex={0} className="flex-1 overflow-y-auto px-4 py-4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-foreground">
        <div className="mx-auto max-w-4xl space-y-5">
          {messages.length === 0 && (
            // Compact empty state — the chat lives in the bottom panel
            // and may share the viewport with the workflow / canvas pane.
            // Vertical padding stays tight so the title and the hint stay
            // inside the visible area at the smallest snap height.
            <div className="flex flex-col items-center justify-center text-center">
              <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl bg-surface-inset">
                <HugeiconsIcon icon={ChatBotIcon} className="h-5 w-5 text-muted-foreground" size="100%" />
              </div>
              <h2 className="text-sm font-semibold text-foreground-strong">
                {t("appTitle")}
              </h2>
              {ontology && suggestions.length > 0 ? (
                <div className="mt-6 grid gap-2 w-full max-w-lg">
                  {suggestions.map((s) => (
                    <button
                      key={`${s.category}-${s.question}`}
                      onClick={() => handleSend(s.question)}
                      className="group flex items-start gap-3 rounded-xl border border-divider bg-surface-base px-4 py-3 text-left text-sm transition-all hover:border-brand-border hover:shadow-sm dark:hover:border-brand-border"
                    >
                      <span className="mt-0.5 shrink-0 rounded-md bg-brand-surface px-1.5 py-0.5 text-2xs font-medium uppercase text-brand-foreground">
                        {s.category}
                      </span>
                      <span className="flex-1 text-foreground group-hover:text-foreground-strong-muted dark:group-hover:text-foreground-strong">
                        {s.question}
                      </span>
                    </button>
                  ))}
                  <button
                    onClick={() => handleSend(t("edaPrompt"))}
                    className="mt-4 rounded-xl border-2 border-dashed border-brand-border bg-brand-surface px-6 py-3 text-sm font-medium text-brand-foreground transition-all hover:border-brand-border hover:bg-brand-surface-strong dark:hover:border-brand-foreground"
                  >
                    {t("runEda")}
                  </button>
                </div>
              ) : (
                // The hint sits in a centred column whose parent
                // `max-w-4xl` already caps the line length. A second
                // narrow cap (`max-w-sm`) forced the Korean copy onto
                // two lines on every viewport ≥ ~600px wide; the
                // outer cap alone gives ~896px which keeps the hint
                // on a single line up to common laptop widths.
                <p className="mt-1 text-xs text-muted-foreground">
                  {ontology
                    ? t("suggestionsHintWithOntology", { count: ontology.node_types.length })
                    : workspaceMode === "analyze"
                      ? t("analyzeLoadHint")
                      : t("designLoadHint")}
                </p>
              )}
              {recentSessions.length > 0 && (
                <div className="mt-6 w-full max-w-lg">
                  <h3 className="mb-2 text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    {t("recentSessions")}
                  </h3>
                  <div className="space-y-1">
                    {recentSessions.map((s) => (
                      <button
                        key={s.id}
                        onClick={() => handleResumeSession(s)}
                        className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors hover:bg-surface-inset"
                      >
                        <span className="min-w-0 flex-1 truncate text-foreground-muted">
                          {s.user_message?.substring(0, 80) || t("untitledSession")}
                          {(s.user_message?.length ?? 0) > 80 ? "..." : ""}
                        </span>
                        <span className="shrink-0 text-2xs text-muted-foreground">
                          {new Date(s.created_at).toLocaleDateString()}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
          <AnimatePresence initial={false}>
            {messages.map((msg) => (
              <motion.div
                key={msg.id}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.2, ease: "easeOut" }}
              >
                <MessageBubble message={msg} onSend={handleSend} />
              </motion.div>
            ))}
          </AnimatePresence>
        </div>
      </div>
      <ChatInput
        onSend={handleSend}
        disabled={inputDisabled}
        disabledReason={disabledReason}
      />
    </div>
    </ErrorBoundary>
  );
}

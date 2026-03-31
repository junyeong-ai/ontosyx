"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useAppStore, type ChatMessage } from "@/lib/store";
import { chatStream, fetchSessionMessages, listAgentSessions, rawQuery, suggestInsights, type InsightSuggestion } from "@/lib/api";
import type { AgentSession } from "@/types/api";
import { errorMessage } from "@/lib/error-messages";
import { toast } from "sonner";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { ChatInput } from "./chat-input";
import { MessageBubble } from "./message-bubble";
import { motion, AnimatePresence } from "motion/react";
import { HugeiconsIcon } from "@hugeicons/react";
import { AiNetworkIcon } from "@hugeicons/core-free-icons";

export function ChatPanel() {
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
    workspaceMode,
    savedOntologyId,
  } = useAppStore();

  const scrollRef = useRef<HTMLDivElement>(null);
  const userScrolledUpRef = useRef(false);
  const abortRef = useRef<AbortController | null>(null);
  const [suggestions, setSuggestions] = useState<InsightSuggestion[]>([]);
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

  // Load insight suggestions when ontology is present and chat is empty
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
  }, [ontology?.id, messages.length]);

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
        toast.success("Session resumed");
      } catch {
        toast.error("Failed to resume session");
      }
    },
    [setSessionId],
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
            content: `Query returned ${result.rows.length} rows`,
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
            saved_ontology_id: isDesignMode
              ? (activeProject?.saved_ontology_id ?? undefined)
              : (savedOntologyId ?? undefined),
            project_id: isDesignMode ? activeProject?.id : undefined,
            project_revision: isDesignMode ? activeProject?.revision : undefined,
            session_id: sessionId ?? undefined,
            execution_mode: getState().executionMode,
            model_override: getState().modelOverride ?? undefined,
          },
          {
            onText(delta) {
              updateMessage(assistantId, {
                content: (getAssistant()?.content ?? "") + delta,
              });
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
            onToolComplete(event) {
              updateMessage(assistantId, {
                toolCalls: (getAssistant()?.toolCalls ?? []).map((tc) =>
                  tc.id === event.id
                    ? { ...tc, output: event.output, status: event.is_error ? "error" : "done", durationMs: event.duration_ms }
                    : tc,
                ),
              });
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
              toast.warning("이전 세션이 만료되어 새 세션으로 시작합니다.");

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
          error: `Connection error: ${err instanceof Error ? err.message : String(err)}`,
          isStreaming: false,
        });
      } finally {
        setIsLoading(false);
      }
    },
    [ontology, activeProject, addMessage, updateMessage, setIsLoading],
  );

  const inputDisabled = isLoading || !ontology;
  const disabledReason = !ontology ? "Load an ontology first" : undefined;

  return (
    <ErrorBoundary name="Chat">
    <div className="flex h-full flex-col bg-zinc-50/50 dark:bg-zinc-950">
      <div ref={scrollRef} role="log" aria-label="Chat messages" aria-live="polite" className="flex-1 overflow-y-auto px-4 py-6">
        <div className="mx-auto max-w-4xl space-y-5">
          {messages.length === 0 && (
            <div className="flex flex-col items-center justify-center pt-16 text-center">
              <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-zinc-100 dark:bg-zinc-800">
                <HugeiconsIcon icon={AiNetworkIcon} className="h-7 w-7 text-zinc-400" size="100%" />
              </div>
              <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">
                Ontosyx AI
              </h2>
              {ontology && suggestions.length > 0 ? (
                <div className="mt-6 grid gap-2 w-full max-w-lg">
                  {suggestions.map((s) => (
                    <button
                      key={`${s.category}-${s.question}`}
                      onClick={() => handleSend(s.question)}
                      className="group flex items-start gap-3 rounded-xl border border-zinc-200 bg-white px-4 py-3 text-left text-sm transition-all hover:border-emerald-300 hover:shadow-sm dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-emerald-700"
                    >
                      <span className="mt-0.5 shrink-0 rounded-md bg-emerald-50 px-1.5 py-0.5 text-[10px] font-medium uppercase text-emerald-600 dark:bg-emerald-950 dark:text-emerald-400">
                        {s.category}
                      </span>
                      <span className="flex-1 text-zinc-700 group-hover:text-zinc-900 dark:text-zinc-300 dark:group-hover:text-zinc-100">
                        {s.question}
                      </span>
                    </button>
                  ))}
                  <button
                    onClick={() => handleSend("이 온톨로지의 데이터에 대해 탐색적 데이터 분석(EDA)을 수행해주세요. 노드 분포, 관계 패턴, 이상치, 핵심 인사이트를 포함해주세요.")}
                    className="mt-4 rounded-xl border-2 border-dashed border-emerald-300 bg-emerald-50/50 px-6 py-3 text-sm font-medium text-emerald-700 transition-all hover:border-emerald-400 hover:bg-emerald-100/50 dark:border-emerald-700 dark:bg-emerald-950/20 dark:text-emerald-400 dark:hover:border-emerald-600"
                  >
                    Run Exploratory Data Analysis
                  </button>
                </div>
              ) : (
                <p className="mt-1.5 max-w-sm text-sm text-zinc-500">
                  {ontology
                    ? `Ask questions, edit the ontology, or explore your knowledge graph with ${ontology.node_types.length} node types.`
                    : workspaceMode === "analyze"
                      ? "Load an ontology in Design mode to enable AI-powered analysis. The agent can query your graph, run analyses, and generate visualizations."
                      : "Load an ontology to start chatting. The AI can help you edit, explain, and analyze your knowledge graph."}
                </p>
              )}
              {recentSessions.length > 0 && (
                <div className="mt-6 w-full max-w-lg">
                  <h3 className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                    Recent Sessions
                  </h3>
                  <div className="space-y-1">
                    {recentSessions.map((s) => (
                      <button
                        key={s.id}
                        onClick={() => handleResumeSession(s)}
                        className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors hover:bg-zinc-100 dark:hover:bg-zinc-800"
                      >
                        <span className="min-w-0 flex-1 truncate text-zinc-600 dark:text-zinc-300">
                          {s.user_message?.substring(0, 80) || "Untitled session"}
                          {(s.user_message?.length ?? 0) > 80 ? "..." : ""}
                        </span>
                        <span className="shrink-0 text-[10px] text-zinc-400">
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

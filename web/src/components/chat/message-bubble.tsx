"use client";

import { useState } from "react";
import { useAppStore, type ChatMessage } from "@/lib/store";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  BotIcon,
  UserIcon,
  ThumbsUpIcon,
  ThumbsDownIcon,
} from "@hugeicons/core-free-icons";
import { Alert } from "@/components/ui/alert";
import { CopyButton } from "@/components/ui/copy-button";
import { Streamdown } from "streamdown";
import { code } from "@streamdown/code";
import { streamdownComponents } from "@/components/chat/streamdown-components";
import { ThinkingBlock } from "@/components/chat/thinking-block";
import { ToolCallCard } from "@/components/chat/tool-call-card";
import { FeedbackButtons } from "@/components/chat/feedback-buttons";
import type { ToolCall } from "@/lib/store";

// ---------------------------------------------------------------------------
// Tool call grouping — collapse consecutive failures of the same tool
// ---------------------------------------------------------------------------

interface ToolCallGroup {
  type: "single" | "collapsed";
  items: ToolCall[];
}

function groupToolCalls(toolCalls: ToolCall[]): ToolCallGroup[] {
  const groups: ToolCallGroup[] = [];
  let i = 0;
  while (i < toolCalls.length) {
    const tc = toolCalls[i];
    if (tc.status === "error") {
      // Collect consecutive failures of the same tool name
      const failedBatch: ToolCall[] = [tc];
      let j = i + 1;
      while (j < toolCalls.length && toolCalls[j].status === "error" && toolCalls[j].name === tc.name) {
        failedBatch.push(toolCalls[j]);
        j++;
      }
      groups.push(
        failedBatch.length >= 2
          ? { type: "collapsed", items: failedBatch }
          : { type: "single", items: failedBatch },
      );
      i = j;
    } else {
      groups.push({ type: "single", items: [tc] });
      i++;
    }
  }
  return groups;
}

// ---------------------------------------------------------------------------
// MessageBubble — renders a single chat message
// ---------------------------------------------------------------------------

interface MessageBubbleProps {
  message: ChatMessage;
  onSend?: (text: string) => void;
}

export function MessageBubble({ message, onSend }: MessageBubbleProps) {
  if (message.role === "user") {
    return (
      <div role="article" aria-label="User message" className="flex flex-row-reverse gap-3">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-zinc-800 text-white dark:bg-zinc-200 dark:text-zinc-800">
          <HugeiconsIcon icon={UserIcon} className="h-4 w-4" size="100%" />
        </div>
        <div className="flex max-w-[80%] justify-end">
          <div className="rounded-2xl bg-zinc-800 px-4 py-2.5 text-sm leading-relaxed text-white dark:bg-zinc-200 dark:text-zinc-900">
            <p className="whitespace-pre-wrap text-left">{message.content}</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div role="article" aria-label="Assistant message" className="flex gap-3">
      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-emerald-100 text-emerald-700 dark:bg-emerald-900/60 dark:text-emerald-400">
        <HugeiconsIcon icon={BotIcon} className="h-4 w-4" size="100%" />
      </div>

      <div className="min-w-0 flex-1 space-y-2">
        {/* Error */}
        {message.error && (
          <Alert variant="error" title="Request failed">
            {message.error}
          </Alert>
        )}

        {/* Chain-of-Thought / Thinking block */}
        {message.thinking && <ThinkingBlock content={message.thinking} isStreaming={message.isStreaming} />}

        {/* Tool calls — group consecutive failures to reduce clutter */}
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="space-y-2">
            {groupToolCalls(message.toolCalls).map((group) =>
              group.type === "single" ? (
                <ToolCallCard key={group.items[0].id} toolCall={group.items[0]} />
              ) : (
                <details key={group.items[0].id} className="rounded-lg border border-red-200/40 bg-red-50/10 dark:border-red-900/30 dark:bg-red-950/10">
                  <summary className="flex cursor-pointer items-center gap-2 px-3 py-2 text-xs text-red-600 dark:text-red-400">
                    <span className="font-medium">{group.items[0].name}</span>
                    <span className="rounded bg-red-100 px-1.5 py-0.5 text-[10px] font-medium dark:bg-red-900/40">
                      failed ×{group.items.length}
                    </span>
                  </summary>
                  <div className="space-y-1 px-2 pb-2">
                    {group.items.map((tc) => (
                      <ToolCallCard key={tc.id} toolCall={tc} />
                    ))}
                  </div>
                </details>
              )
            )}
          </div>
        )}

        {/* Text content */}
        {(message.content || message.isStreaming) && !message.error && (
          <div className="group/msg relative max-w-none rounded-2xl bg-white px-4 py-2.5 text-sm leading-relaxed text-zinc-800 shadow-sm ring-1 ring-zinc-100 dark:bg-zinc-800 dark:text-zinc-100 dark:ring-zinc-700">
            {/* Copy button — visible on hover */}
            {message.content && !message.isStreaming && (
              <div className="opacity-0 group-hover/msg:opacity-100 transition-opacity">
                <CopyButton text={message.content} />
              </div>
            )}
            {message.content ? (
              <div className="prose-message">
                <Streamdown
                  plugins={{ code }}
                  components={streamdownComponents}
                  mode={message.isStreaming ? "streaming" : "static"}
                  caret={message.isStreaming ? "block" : undefined}
                  animated={message.isStreaming ? { animation: "fadeIn" } : false}
                  controls={false}
                >{message.content}</Streamdown>
              </div>
            ) : null}
            {message.isStreaming && !message.content && !message.thinking && noToolsRunning(message) && (
              <div className="flex items-center gap-1 py-1">
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-400 [animation-delay:0ms]" />
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-400 [animation-delay:150ms]" />
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-400 [animation-delay:300ms]" />
              </div>
            )}
            {/* Streaming caret handled by Streamdown's built-in caret prop */}
          </div>
        )}

        {/* Follow-up suggestions */}
        {!message.isStreaming && message.role === "assistant" && message.content && (
          <SuggestedFollowups content={message.content} onSend={onSend} />
        )}

        {/* General message feedback */}
        {!message.isStreaming && message.role === "assistant" && message.content && !message.error && (
          hasQueryResult(message) ? (
            <div className="flex items-center gap-1">
              <FeedbackButtons executionId={getExecutionId(message)} />
            </div>
          ) : (
            <MessageFeedback messageId={message.id} />
          )
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// SuggestedFollowups — clickable follow-up question chips
// ---------------------------------------------------------------------------

/** Strip markdown formatting (bold, italic, backticks) for clean button text. */
function stripMarkdown(text: string): string {
  return text.replace(/\*\*|__/g, "").replace(/\*|_/g, "").replace(/`/g, "").trim();
}

/** Strip common bullet prefixes: `- `, `* `, `• `, `1. `, emoji bullets, `> ` */
function stripBullet(line: string): string {
  return line
    .replace(/^(?:[-*•●▸▹▪▫◦‣]\s+|\d+[.)]\s+|>\s+)/, "")  // standard bullets
    .replace(/^[\p{Emoji_Presentation}\p{Emoji}\u200d]+\s*/u, "")  // emoji prefix
    .trim();
}

function SuggestedFollowups({ content, onSend }: { content: string; onSend?: (text: string) => void }) {
  // Extract trailing question lines from assistant content.
  // Supports: `- Q?`, `• Q?`, `* Q?`, `1. Q?`, emoji-prefixed, `> Q?`
  const lines = content.trim().split("\n");
  const questions: string[] = [];
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i].trim();
    if (!line || line === "---") {
      continue;
    }
    const stripped = stripBullet(line);
    if (stripped.endsWith("?") && stripped.length > 10) {
      questions.unshift(stripMarkdown(stripped));
    } else {
      break;
    }
  }

  if (questions.length === 0) return null;

  const handleClick = (q: string) => {
    if (onSend) {
      onSend(q);
    } else {
      const store = useAppStore.getState();
      store.setWorkspaceMode("analyze");
      store.setCommandBarInput(q);
    }
  };

  return (
    <div className="mt-3 max-w-[80%] space-y-1">
      {questions.slice(0, 3).map((q, i) => (
        <button
          key={q}
          onClick={() => handleClick(q)}
          className="block w-full text-left text-sm text-emerald-400 hover:text-emerald-300 hover:underline"
        >
          <span className="mr-2 text-zinc-500">{i + 1}.</span>
          {q}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// MessageFeedback — local-only thumbs up/down for general assistant messages
// ---------------------------------------------------------------------------

function MessageFeedback({ messageId }: { messageId: string }) {
  const [feedback, setFeedback] = useState<"positive" | "negative" | null>(null);

  const toggle = (value: "positive" | "negative") => {
    setFeedback(feedback === value ? null : value);
  };

  return (
    <div className="flex items-center gap-0.5 mt-1">
      <button
        onClick={() => toggle("positive")}
        className={`rounded p-1 text-xs transition-colors ${
          feedback === "positive"
            ? "text-emerald-500"
            : "text-zinc-300 hover:text-zinc-500 dark:text-zinc-600 dark:hover:text-zinc-400"
        }`}
        aria-label={feedback === "positive" ? "Remove helpful rating" : "Helpful"}
      >
        <HugeiconsIcon icon={ThumbsUpIcon} className="h-3 w-3" size="100%" />
      </button>
      <button
        onClick={() => toggle("negative")}
        className={`rounded p-1 text-xs transition-colors ${
          feedback === "negative"
            ? "text-red-500"
            : "text-zinc-300 hover:text-zinc-500 dark:text-zinc-600 dark:hover:text-zinc-400"
        }`}
        aria-label={feedback === "negative" ? "Remove unhelpful rating" : "Not helpful"}
      >
        <HugeiconsIcon icon={ThumbsDownIcon} className="h-3 w-3" size="100%" />
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function noToolsRunning(message: ChatMessage): boolean {
  return !(message.toolCalls ?? []).some((tc) => tc.status === "running");
}

function hasQueryResult(message: ChatMessage): boolean {
  return (message.toolCalls ?? []).some(
    (tc) => tc.name === "query_graph" && tc.status === "done" && tc.output,
  );
}

function getExecutionId(message: ChatMessage): string {
  const tc = (message.toolCalls ?? []).find(
    (tc) => tc.name === "query_graph" && tc.status === "done",
  );
  if (!tc?.output) return "";
  try {
    const parsed = JSON.parse(tc.output);
    return parsed.execution_id ?? "";
  } catch {
    return "";
  }
}

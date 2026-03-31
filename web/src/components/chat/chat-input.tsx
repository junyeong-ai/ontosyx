"use client";

import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { ArrowUp01Icon } from "@hugeicons/core-free-icons";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { useAppStore } from "@/lib/store";
import { request } from "@/lib/api/client";
import type { ModelConfig } from "@/lib/api/models";

function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return String(n);
}

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  disabledReason?: string;
}

export function ChatInput({
  onSend,
  disabled,
  disabledReason,
}: ChatInputProps) {
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const tokenUsage = useAppStore((s) => s.tokenUsage);
  const executionMode = useAppStore((s) => s.executionMode);
  const modelOverride = useAppStore((s) => s.modelOverride);
  const setModelOverride = useAppStore((s) => s.setModelOverride);
  const [models, setModels] = useState<ModelConfig[]>([]);

  useEffect(() => {
    request<ModelConfig[]>("/models/configs")
      .then((configs) => setModels(configs.filter((c) => c.enabled)))
      .catch(() => {
        // Silent — model selector is optional
      });
  }, []);

  const handleSend = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setValue("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [value, disabled, onSend]);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInput = () => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 200) + "px";
  };

  const isRawMode = value.startsWith("!");
  const placeholder = disabledReason
    ? disabledReason
    : "Ask anything... (prefix ! for raw Cypher)";

  const canSend = !disabled && value.trim().length > 0;

  return (
    <div className="border-t border-zinc-200 bg-white px-4 py-3 dark:border-zinc-800 dark:bg-zinc-950">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <div className="relative flex-1">
          <textarea
            ref={textareaRef}
            data-chat-input
            aria-label="Chat message"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={handleKeyDown}
            onInput={handleInput}
            placeholder={placeholder}
            rows={1}
            disabled={disabled}
            className={cn(
              "w-full resize-none rounded-xl border border-zinc-200 bg-zinc-50 px-4 py-3 pr-12",
              "text-sm placeholder:text-zinc-500",
              "focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-2 focus:ring-emerald-500/50",
              "dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-100",
              "dark:focus:border-emerald-400 dark:focus:bg-zinc-900 dark:focus:ring-emerald-400/50",
              "disabled:opacity-50 disabled:cursor-not-allowed",
              "transition-all",
            )}
          />
          {disabled && disabledReason ? (
            <Tooltip content={disabledReason}>
              <button
                disabled
                className="absolute right-2.5 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-lg bg-zinc-200 text-zinc-400 dark:bg-zinc-700 dark:text-zinc-500"
                aria-label={disabledReason}
              >
                <HugeiconsIcon icon={ArrowUp01Icon} className="h-3.5 w-3.5" size="100%" strokeWidth={2.5} />
              </button>
            </Tooltip>
          ) : (
            <button
              onClick={handleSend}
              disabled={!canSend}
              className={cn(
                "absolute right-2.5 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-lg transition-all",
                canSend
                  ? "bg-emerald-600 text-white shadow-sm hover:bg-emerald-700"
                  : "bg-zinc-200 text-zinc-400 dark:bg-zinc-700 dark:text-zinc-500",
              )}
              aria-label="Send message"
            >
              <HugeiconsIcon icon={ArrowUp01Icon} className="h-3.5 w-3.5" size="100%" strokeWidth={2.5} />
            </button>
          )}
        </div>
      </div>
      <div className="mx-auto mt-1.5 flex max-w-3xl items-center gap-2 text-[10px] text-zinc-400">
        <span>
          {isRawMode ? (
            <span className="text-amber-500">Raw Cypher mode</span>
          ) : (
            "Enter to send, Shift+Enter for new line"
          )}
          {tokenUsage && (
            <span className="ml-2">
              {formatTokens(tokenUsage.input + tokenUsage.output)} tokens used
            </span>
          )}
        </span>
        <span className="flex-1" />
        {models.length > 0 && (
          <select
            value={modelOverride ?? ""}
            onChange={(e) => setModelOverride(e.target.value || null)}
            className="rounded-md border border-zinc-200 bg-zinc-50 px-1.5 py-0.5 text-[10px] text-zinc-500 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400"
            title="Model override"
          >
            <option value="">Default model</option>
            {models.map((m) => (
              <option key={m.id} value={m.model_id}>
                {m.name} ({m.model_id})
              </option>
            ))}
          </select>
        )}
        <button
          onClick={() => {
            const store = useAppStore.getState();
            store.setExecutionMode(store.executionMode === "auto" ? "supervised" : "auto");
          }}
          className={cn(
            "rounded-md px-2 py-0.5 text-[10px] font-medium transition-colors",
            executionMode === "supervised"
              ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
              : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
          )}
          title={executionMode === "auto" ? "Auto mode: tools execute automatically" : "Supervised: tools require approval"}
        >
          {executionMode === "auto" ? "Auto" : "Supervised"}
        </button>
      </div>
    </div>
  );
}

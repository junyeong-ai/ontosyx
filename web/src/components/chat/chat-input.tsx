"use client";

import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";
import { useTranslations } from "next-intl";
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
  const t = useTranslations("workbench.chat.input");
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
  const placeholder = disabledReason ? disabledReason : t("placeholder");

  const canSend = !disabled && value.trim().length > 0;

  return (
    <div className="border-t border-divider bg-surface-base px-4 py-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <div className="relative flex-1">
          <textarea
            ref={textareaRef}
            data-chat-input
            aria-label={t("messageAria")}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={handleKeyDown}
            onInput={handleInput}
            placeholder={placeholder}
            rows={1}
            disabled={disabled}
            className={cn(
              "w-full resize-none rounded-xl border border-divider bg-surface-raised px-4 py-3 pr-12",
              "text-sm placeholder:text-muted-foreground",
              "focus:border-brand-foreground focus:bg-surface-base focus:outline-none focus:ring-2 focus:ring-brand-foreground/50",
              "dark:border-divider-strong",
              "dark:focus:border-brand-border dark:focus:ring-brand-foreground/50",
              "disabled:opacity-50 disabled:cursor-not-allowed",
              "transition-all",
            )}
          />
          {disabled && disabledReason ? (
            <Tooltip content={disabledReason}>
              <button
                disabled
                className="absolute right-2.5 top-1 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-lg bg-surface-inset text-muted-foreground"
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
                "absolute right-2.5 top-1 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-lg transition-all",
                canSend
                  ? "bg-brand-solid text-white shadow-sm hover:bg-brand-solid"
                  : "bg-surface-inset text-muted-foreground",
              )}
              aria-label={t("sendAria")}
            >
              <HugeiconsIcon icon={ArrowUp01Icon} className="h-3.5 w-3.5" size="100%" strokeWidth={2.5} />
            </button>
          )}
        </div>
      </div>
      <div className="mx-auto mt-1.5 flex max-w-3xl items-center gap-2 text-2xs text-muted-foreground">
        <span>
          {isRawMode ? (
            <span className="text-warning-foreground">{t("rawMode")}</span>
          ) : (
            t("enterHint")
          )}
          {tokenUsage && (
            <span className="ml-2">
              {t("tokensUsed", { count: formatTokens(tokenUsage.input + tokenUsage.output) })}
            </span>
          )}
        </span>
        <span className="flex-1" />
        {models.length > 0 && (
          <select
            value={modelOverride ?? ""}
            onChange={(e) => setModelOverride(e.target.value || null)}
            className="rounded-md border border-divider bg-surface-raised px-1.5 py-0.5 text-2xs text-foreground-muted dark:text-muted-foreground"
            title={t("modelSelectTitle")}
          >
            <option value="">{t("defaultModel")}</option>
            {models.map((m) => (
              <option key={m.id} value={m.model_id}>
                {t("modelOption", { name: m.name, id: m.model_id })}
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
            "rounded-md px-2 py-0.5 text-2xs font-medium transition-colors",
            executionMode === "supervised"
              ? "bg-warning-surface text-warning-foreground"
              : "text-muted-foreground hover:text-foreground dark:hover:text-foreground-muted"
          )}
          title={executionMode === "auto" ? t("executionMode.autoTitle") : t("executionMode.supervisedTitle")}
        >
          {executionMode === "auto" ? t("executionMode.auto") : t("executionMode.supervised")}
        </button>
      </div>
    </div>
  );
}

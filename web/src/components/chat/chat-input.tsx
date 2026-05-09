"use client";

import { useState, useRef, useCallback, useEffect, type KeyboardEvent } from "react";
import { useTranslations } from "next-intl";
import { ArrowUp, Square } from "lucide-react";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { ChatComposer, FormSelect } from "@/components/ui/form-input";
import { useAppStore } from "@/lib/store";
import { request } from "@/lib/api/client";
import type { ModelConfig } from "@/lib/api/models";

function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return String(n);
}

interface ChatInputProps {
  onSend: (message: string) => void;
  /**
   * When true, the trailing button switches from "send" to "stop"
   * and clicking it cancels the in-flight stream via `onStop`. The
   * caller owns the AbortController; this component just exposes
   * the affordance — without one, a long completion lingers with
   * no user-facing way out.
   */
  isStreaming?: boolean;
  onStop?: () => void;
  disabled?: boolean;
  disabledReason?: string;
}

export function ChatInput({
  onSend,
  isStreaming,
  onStop,
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

  const isRawMode = value.startsWith("!");
  const placeholder = disabledReason ? disabledReason : t("placeholder");

  const canSend = !disabled && value.trim().length > 0;

  // Trailing slot owns the three button states: stop while streaming,
  // disabled-with-reason, and idle send. ChatComposer handles the
  // textarea sizing + overlay positioning so the surface stays
  // the inverse-shape (textarea on left, button on right) every time.
  const trailing =
    isStreaming && onStop ? (
      <Tooltip content={t("stopAria")}>
        <button
          type="button"
          onClick={onStop}
          className="flex h-7 w-7 items-center justify-center rounded-lg bg-danger-solid text-foreground-on-accent shadow-1 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] hover:bg-danger-solid-hover focus-visible:ring-2 focus-visible:ring-danger-foreground/40"
          aria-label={t("stopAria")}
        >
          <Square className="h-3 w-3" strokeWidth={1.5} />
        </button>
      </Tooltip>
    ) : disabled && disabledReason ? (
      <Tooltip content={disabledReason}>
        <button
          type="button"
          disabled
          className="flex h-7 w-7 items-center justify-center rounded-lg bg-surface-inset text-foreground-muted"
          aria-label={disabledReason}
        >
          <ArrowUp className="h-3.5 w-3.5" strokeWidth={1.5} />
        </button>
      </Tooltip>
    ) : (
      <button
        type="button"
        onClick={handleSend}
        disabled={!canSend}
        className={cn(
          "flex h-7 w-7 items-center justify-center rounded-lg transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
          canSend
            ? "bg-brand-solid text-foreground-onbrand shadow-1 hover:bg-brand-solid"
            : "bg-surface-inset text-foreground-muted",
        )}
        aria-label={t("sendAria")}
      >
        <ArrowUp className="h-3.5 w-3.5" strokeWidth={1.5} />
      </button>
    );

  return (
    <div className="border-t border-divider bg-surface-base px-4 py-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <ChatComposer
          ref={textareaRef}
          data-chat-input
          aria-label={t("messageAria")}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          disabled={disabled}
          trailing={trailing}
        />
      </div>
      <div className="mx-auto mt-1.5 flex max-w-3xl items-center gap-2 text-2xs text-foreground-muted">
        <span>
          {isRawMode ? (
            <span className="text-warning-foreground">{t("rawMode")}</span>
          ) : (
            t("enterHint")
          )}
          {tokenUsage && (
            <span className="ms-2">
              {t("tokensUsed", { count: formatTokens(tokenUsage.input + tokenUsage.output) })}
            </span>
          )}
        </span>
        <span className="flex-1" />
        {models.length > 0 && (
          <FormSelect
            density="compact"
            value={modelOverride ?? ""}
            onChange={(e) => setModelOverride(e.target.value || null)}
            aria-label={t("modelSelectTitle")}
            title={t("modelSelectTitle")}
            className="bg-surface-raised text-foreground-muted"
          >
            <option value="">{t("defaultModel")}</option>
            {models.map((m) => (
              <option key={m.id} value={m.model_id}>
                {t("modelOption", { name: m.name, id: m.model_id })}
              </option>
            ))}
          </FormSelect>
        )}
        <button
          type="button"
          onClick={() => {
            const store = useAppStore.getState();
            store.setExecutionMode(store.executionMode === "auto" ? "supervised" : "auto");
          }}
          className={cn(
            "rounded-md px-2 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
            executionMode === "supervised"
              ? "bg-warning-surface text-warning-foreground"
              : "text-foreground-muted hover:text-foreground-muted"
          )}
          title={executionMode === "auto" ? t("executionMode.autoTitle") : t("executionMode.supervisedTitle")}
        >
          {executionMode === "auto" ? t("executionMode.auto") : t("executionMode.supervised")}
        </button>
      </div>
    </div>
  );
}

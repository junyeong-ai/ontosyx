"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  AiNetworkIcon,
  ArrowDown01Icon,
  ArrowUp01Icon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";

// ---------------------------------------------------------------------------
// ThinkingBlock — collapsible chain-of-thought reasoning
// ---------------------------------------------------------------------------

interface ThinkingBlockProps {
  content: string;
  isStreaming?: boolean;
}

export function ThinkingBlock({ content, isStreaming }: ThinkingBlockProps) {
  const t = useTranslations("workbench.chat.thinking");
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="rounded-xl border border-warning-border/60 bg-warning-surface/20">
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex w-full items-center gap-2 px-3 py-2 text-start text-xs text-warning-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-warning-surface"
      >
        {isStreaming ? (
          <Spinner size="sm" className="text-warning-foreground" />
        ) : (
          <HugeiconsIcon icon={AiNetworkIcon} className="h-3.5 w-3.5" size="100%" />
        )}
        <span className="font-medium">
          {isStreaming && !content ? t("thinking") : t("reasoning")}
        </span>
        <span className="ms-auto text-2xs text-warning-foreground">
          {content.length > 0 && t("steps", { count: content.split("\n").length })}
        </span>
        <HugeiconsIcon
          icon={isOpen ? ArrowUp01Icon : ArrowDown01Icon}
          className="h-3 w-3 text-warning-foreground"
          size="100%"
        />
      </button>
      {isOpen && (
        <div className="border-t border-warning-border/40 px-3 py-2">
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-2xs leading-relaxed text-warning-foreground">
            {content}
            {isStreaming && <span className="ms-0.5 inline-block h-3 w-0.5 animate-blink bg-warning-foreground align-text-bottom" />}
          </pre>
        </div>
      )}
    </div>
  );
}

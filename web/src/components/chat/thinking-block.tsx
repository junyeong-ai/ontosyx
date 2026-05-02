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
        onClick={() => setIsOpen(!isOpen)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-warning-foreground transition-colors hover:bg-warning-surface dark:hover:bg-warning-surface/30"
      >
        {isStreaming ? (
          <Spinner size="sm" className="text-warning-foreground" />
        ) : (
          <HugeiconsIcon icon={AiNetworkIcon} className="h-3.5 w-3.5" size="100%" />
        )}
        <span className="font-medium">
          {isStreaming && !content ? t("thinking") : t("reasoning")}
        </span>
        <span className="ml-auto text-2xs text-warning-foreground">
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
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-[11px] leading-relaxed text-warning-foreground/70">
            {content}
            {isStreaming && <span className="ml-0.5 inline-block h-3 w-0.5 animate-blink bg-warning-foreground align-text-bottom" />}
          </pre>
        </div>
      )}
    </div>
  );
}

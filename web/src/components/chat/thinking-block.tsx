"use client";

import { useState } from "react";
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
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="rounded-xl border border-amber-200/60 bg-amber-50/40 dark:border-amber-800/40 dark:bg-amber-950/20">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-amber-700 transition-colors hover:bg-amber-50/60 dark:text-amber-400 dark:hover:bg-amber-950/30"
      >
        {isStreaming ? (
          <Spinner size="sm" className="text-amber-500" />
        ) : (
          <HugeiconsIcon icon={AiNetworkIcon} className="h-3.5 w-3.5" size="100%" />
        )}
        <span className="font-medium">
          {isStreaming && !content ? "Thinking..." : "Reasoning"}
        </span>
        <span className="ml-auto text-[10px] text-amber-500/70">
          {content.length > 0 && `${content.split("\n").length} steps`}
        </span>
        <HugeiconsIcon
          icon={isOpen ? ArrowUp01Icon : ArrowDown01Icon}
          className="h-3 w-3 text-amber-400"
          size="100%"
        />
      </button>
      {isOpen && (
        <div className="border-t border-amber-200/40 px-3 py-2 dark:border-amber-800/30">
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-[11px] leading-relaxed text-amber-800/80 dark:text-amber-300/70">
            {content}
            {isStreaming && <span className="ml-0.5 inline-block h-3 w-0.5 animate-blink bg-amber-500 align-text-bottom" />}
          </pre>
        </div>
      )}
    </div>
  );
}

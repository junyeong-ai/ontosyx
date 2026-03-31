"use client";

import { useEffect, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  CopyIcon,
  Tick01Icon,
} from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// CopyButton
// ---------------------------------------------------------------------------

interface CopyButtonProps {
  text: string;
  /** "absolute" (default, positioned top-right) or "inline" (flow with siblings) */
  variant?: "absolute" | "inline";
}

export function CopyButton({ text, variant = "absolute" }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
  };

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  return (
    <button
      onClick={handleCopy}
      className={`rounded p-1 text-zinc-400 transition-colors hover:bg-zinc-200 hover:text-zinc-600 focus-visible:ring-2 focus-visible:ring-emerald-500/50 focus-visible:outline-none dark:hover:bg-zinc-700 dark:hover:text-zinc-300 ${
        variant === "absolute" ? "absolute right-2 top-2" : ""
      }`}
      aria-label="Copy to clipboard"
      title={copied ? "Copied!" : "Copy"}
    >
      {copied ? (
        <HugeiconsIcon icon={Tick01Icon} className="h-3.5 w-3.5 text-emerald-500" size="100%" />
      ) : (
        <HugeiconsIcon icon={CopyIcon} className="h-3.5 w-3.5" size="100%" />
      )}
    </button>
  );
}

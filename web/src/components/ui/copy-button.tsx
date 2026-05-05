"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { Check, Copy } from "lucide-react";
import { cn } from "@/lib/cn";

interface CopyButtonProps {
  text: string;
  /** "absolute" (default, positioned top-right) or "inline" (flow with siblings) */
  variant?: "absolute" | "inline";
}

export function CopyButton({ text, variant = "absolute" }: CopyButtonProps) {
  const t = useTranslations("common.copyButton");
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
    <button type="button"
      onClick={handleCopy}
      className={cn(
        "cursor-pointer rounded p-1 text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        "hover:bg-surface-inset hover:text-foreground",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-1",
        variant === "absolute" && "absolute end-2 top-2",
      )}
      aria-label={t("ariaLabel")}
      title={copied ? t("copied") : t("copy")}
    >
      {copied ? (
        <Check className="h-3.5 w-3.5 text-success-foreground" />
      ) : (
        <Copy className="h-3.5 w-3.5" />
      )}
    </button>
  );
}

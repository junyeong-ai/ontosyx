"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { setQueryFeedback } from "@/lib/api";
import { ThumbsDown, ThumbsUp } from "lucide-react";
// ---------------------------------------------------------------------------
// FeedbackButtons — toggleable thumbs up/down for query results
// ---------------------------------------------------------------------------

interface FeedbackButtonsProps {
  executionId: string;
}

export function FeedbackButtons({ executionId }: FeedbackButtonsProps) {
  const t = useTranslations("workbench.chat.feedback");
  const [feedback, setFeedback] = useState<"positive" | "negative" | null>(null);
  const [saving, setSaving] = useState(false);

  if (!executionId) return null;

  const handleFeedback = async (value: "positive" | "negative") => {
    if (saving) return;
    const next = feedback === value ? null : value;
    setFeedback(next);
    setSaving(true);
    try {
      await setQueryFeedback(executionId, next);
    } catch {
      setFeedback(feedback); // revert on error
      toast.error(t("toast.saveFailed"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex items-center gap-0.5">
      <button
        type="button"
        onClick={() => handleFeedback("positive")}
        disabled={saving}
        className={`rounded p-1 text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
          feedback === "positive"
            ? "text-brand-foreground"
            : "text-foreground-muted hover:bg-surface-inset hover:text-foreground-muted"
        } disabled:cursor-wait`}
        aria-label={t("good")}
      >
        <ThumbsUp className="h-3 w-3" />
      </button>
      <button
        type="button"
        onClick={() => handleFeedback("negative")}
        disabled={saving}
        className={`rounded p-1 text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
          feedback === "negative"
            ? "text-danger-foreground"
            : "text-foreground-muted hover:bg-surface-inset hover:text-foreground-muted"
        } disabled:cursor-wait`}
        aria-label={t("bad")}
      >
        <ThumbsDown className="h-3 w-3" />
      </button>
    </div>
  );
}

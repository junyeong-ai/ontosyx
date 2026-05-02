"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";
import { setQueryFeedback } from "@/lib/api";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ThumbsUpIcon,
  ThumbsDownIcon,
} from "@hugeicons/core-free-icons";

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
        onClick={() => handleFeedback("positive")}
        disabled={saving}
        className={`rounded p-1 text-xs transition-colors ${
          feedback === "positive"
            ? "text-brand-foreground"
            : "text-muted-foreground hover:bg-surface-inset hover:text-foreground dark:hover:text-foreground-muted"
        } disabled:cursor-wait`}
        aria-label={t("good")}
      >
        <HugeiconsIcon icon={ThumbsUpIcon} className="h-3 w-3" size="100%" />
      </button>
      <button
        onClick={() => handleFeedback("negative")}
        disabled={saving}
        className={`rounded p-1 text-xs transition-colors ${
          feedback === "negative"
            ? "text-danger-foreground"
            : "text-muted-foreground hover:bg-surface-inset hover:text-foreground dark:hover:text-foreground-muted"
        } disabled:cursor-wait`}
        aria-label={t("bad")}
      >
        <HugeiconsIcon icon={ThumbsDownIcon} className="h-3 w-3" size="100%" />
      </button>
    </div>
  );
}

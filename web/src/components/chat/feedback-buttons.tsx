"use client";

import { useState } from "react";
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
      toast.error("Failed to save feedback");
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
            ? "text-emerald-500"
            : "text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
        } disabled:cursor-wait`}
        aria-label="Good result"
      >
        <HugeiconsIcon icon={ThumbsUpIcon} className="h-3 w-3" size="100%" />
      </button>
      <button
        onClick={() => handleFeedback("negative")}
        disabled={saving}
        className={`rounded p-1 text-xs transition-colors ${
          feedback === "negative"
            ? "text-red-500"
            : "text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
        } disabled:cursor-wait`}
        aria-label="Bad result"
      >
        <HugeiconsIcon icon={ThumbsDownIcon} className="h-3 w-3" size="100%" />
      </button>
    </div>
  );
}

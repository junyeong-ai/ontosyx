"use client";

import { useEffect } from "react";
import { useAppStore } from "@/lib/store";
import type { NotificationTone } from "@/lib/store/types";

/**
 * Operations surface → sidebar badge contract. Each daily-visit
 * mode (`approvals`, `quality`, `evaluation`, `knowledgeBase`)
 * publishes its pending / regression / stale count via this hook;
 * the sidebar entry surfaces a tone-keyed pill (expanded) or dot
 * (rail). `count <= 0` clears the badge.
 *
 * Tone vocabulary stays narrow — `warning` for queues awaiting human
 * action, `danger` for regression / failure signals, `info` for
 * informational counts. Each surface owns the mapping locally.
 */
export function usePublishModeCount(
  modeId: string,
  count: number,
  tone: NotificationTone = "warning",
) {
  const publish = useAppStore((s) => s.publishModeCount);
  const clear = useAppStore((s) => s.clearModeCount);
  useEffect(() => {
    if (count <= 0) {
      clear(modeId);
      return;
    }
    publish(modeId, { count, tone });
  }, [modeId, count, tone, publish, clear]);
}

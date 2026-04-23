"use client";

// React hook wrapping `useQualityMetrics` + alert derivation +
// session-scoped dismissal. Kept in `hooks/` rather than inside the
// `quality/` component folder so non-UI callers (e.g., future agent
// notifications) can reuse the same signature without pulling in
// the banner's component tree.

import { useCallback, useMemo, useState } from "react";

import { useQualityMetrics } from "@/hooks/api/use-quality";
import type { MetricWindow } from "@/lib/api/quality";
import {
  alertSignature,
  computeQualityAlerts,
  type QualityAlert,
} from "@/lib/quality/alerts";

/**
 * sessionStorage key for the dismissal signature. Session-scoped on
 * purpose — closing the tab or refreshing wipes the dismissal so
 * operators aren't stuck in a quiet state after a restart.
 */
const DISMISS_KEY = "ontosyx.quality-banner.dismissed-signature.v1";

function readDismissed(): string | null {
  if (typeof window === "undefined") return null;
  try {
    return window.sessionStorage.getItem(DISMISS_KEY);
  } catch {
    // Private-mode Safari / quota / etc. — treat as not dismissed.
    return null;
  }
}

function writeDismissed(signature: string): void {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.setItem(DISMISS_KEY, signature);
  } catch {
    // Same as read — we just lose the dismissal for this tab.
  }
}

export interface UseQualityAlertsResult {
  /** All alerts currently active, regardless of dismissal. */
  alerts: QualityAlert[];
  /** `true` once metrics have loaded AND alerts are present AND not
   *  dismissed for the current signature. */
  visible: boolean;
  /** Dismiss the current signature for the rest of the session. */
  dismiss: () => void;
  /** Forwarded from TanStack Query so callers can decide on skeletons. */
  isLoading: boolean;
}

/**
 * Subscribe to the quality-metrics endpoint and return an
 * easy-to-render alert object. Stays safe during SSR — reads
 * sessionStorage lazily through `useState(() => ...)`.
 */
export function useQualityAlerts(
  window: MetricWindow = "7d",
): UseQualityAlertsResult {
  const query = useQualityMetrics(window);
  const alerts = useMemo(
    () => (query.data ? computeQualityAlerts(query.data) : []),
    [query.data],
  );

  const signature = useMemo(() => alertSignature(alerts), [alerts]);

  // Snapshot the dismissed signature on first render so downstream
  // `visible` calculation doesn't depend on the async `useEffect`
  // window. The initialiser function ensures SSR safety.
  const [dismissed, setDismissed] = useState<string | null>(readDismissed);

  const visible = alerts.length > 0 && dismissed !== signature;

  const dismiss = useCallback(() => {
    writeDismissed(signature);
    setDismissed(signature);
  }, [signature]);

  return { alerts, visible, dismiss, isLoading: query.isLoading };
}

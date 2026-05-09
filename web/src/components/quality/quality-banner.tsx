"use client";

// QualityBanner — ambient quality-degradation surface.
//
// Shows when `/quality/metrics` surfaces any metric below its
// warning/critical band (see `src/lib/quality/alerts.ts`). Hidden
// on the quality-signals dashboard itself (it would be redundant
// there) and whenever the operator dismisses it for the session.
//
// Composes on top of the shared `<Alert>` component so dismiss +
// icon + color variants stay consistent with the rest of the UI.

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";

import { Alert } from "@/components/ui/alert";
import { useQualityAlerts } from "@/hooks/use-quality-alerts";
import { dominantAlert } from "@/lib/quality/alerts";

const SIGNALS_PATH = "/quality?tab=signals";

export function QualityBanner() {
  const t = useTranslations("qualityBanner");
  const pathname = usePathname();
  const { alerts, visible, dismiss } = useQualityAlerts();

  // No alerts, dismissed, or already on the details page → render
  // nothing. Returning `null` keeps the layout identical when the
  // banner is quiet (no flash-of-banner during hydration).
  if (!visible || pathname === SIGNALS_PATH) return null;

  const headline = dominantAlert(alerts);
  // `visible` already gated on `alerts.length > 0`, so `headline`
  // is non-null here. The `!` lives in the JSX to keep the early
  // return readable.
  const metric = headline!.metric;
  const severity = headline!.severity;
  // Percent display — multiply by 100 and round. Matches the
  // /quality?tab=signals dashboard's tile formatting.
  const valueDisplay = `${(headline!.value * 100).toFixed(0)}%`;
  const thresholdDisplay = `${(headline!.threshold * 100).toFixed(0)}%`;

  const others = alerts.length - 1;

  return (
    <div className="px-4 pt-2">
      <Alert
        variant={severity === "critical" ? "error" : "warning"}
        title={t(`title.${severity}`)}
        onDismiss={dismiss}
      >
        <span>
          {t(`metric.${metric}`, {
            value: valueDisplay,
            threshold: thresholdDisplay,
          })}
        </span>
        {others > 0 && (
          <span className="ms-1">
            {" · "}
            {t("others", { count: others })}
          </span>
        )}
        {" · "}
        <Link
          href={SIGNALS_PATH}
          className="underline underline-offset-2 hover:no-underline"
        >
          {t("viewDetails")}
        </Link>
      </Alert>
    </div>
  );
}

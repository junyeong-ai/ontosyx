"use client";

import { useState, useSyncExternalStore } from "react";
import { useTranslations } from "next-intl";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";

const STORAGE_KEY = "ontosyx.onboarded";

/**
 * Step icons — kept outside the component so the glyph set is
 * locale-independent. Titles and descriptions come from `messages/*.json`
 * via `useTranslations("welcome")`.
 */
const STEP_ICONS = ["\u{1F517}", "\u{1F9E0}", "\u{1F4A1}"] as const;
const STEP_KEYS = ["step1", "step2", "step3"] as const;

/**
 * Subscribe to `localStorage` changes (cross-tab via the native `storage`
 * event; same-tab via the manual dispatch in `dismiss`).
 */
function subscribeToOnboardingStatus(onChange: () => void): () => void {
  window.addEventListener("storage", onChange);
  return () => window.removeEventListener("storage", onChange);
}

function getOnboardedStatus(): boolean {
  return !!localStorage.getItem(STORAGE_KEY);
}

/** SSR: assume already onboarded so the modal never flashes during SSR HTML. */
function getServerOnboardedStatus(): boolean {
  return true;
}

export function WelcomeModal() {
  const [step, setStep] = useState(0);
  const t = useTranslations("welcome");
  const onboarded = useSyncExternalStore(
    subscribeToOnboardingStatus,
    getOnboardedStatus,
    getServerOnboardedStatus,
  );

  if (onboarded) return null;

  const dismiss = () => {
    localStorage.setItem(STORAGE_KEY, "true");
    // `storage` fires only across tabs; dispatch manually so THIS tab's
    // `useSyncExternalStore` re-reads and this modal unmounts on the
    // next render pass.
    window.dispatchEvent(new StorageEvent("storage", { key: STORAGE_KEY }));
  };

  const currentKey = STEP_KEYS[step];
  const currentIcon = STEP_ICONS[step];
  const isLast = step === STEP_KEYS.length - 1;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white p-8 shadow-2xl dark:bg-zinc-900">
        {/* Step indicator */}
        <div className="mb-6 flex justify-center gap-2">
          {STEP_KEYS.map((_, i) => (
            <div
              key={i}
              className={cn(
                "h-1.5 w-8 rounded-full",
                i === step
                  ? "bg-emerald-500"
                  : "bg-zinc-200 dark:bg-zinc-700",
              )}
            />
          ))}
        </div>

        {/* Content */}
        <div className="text-center">
          <span className="text-4xl">{currentIcon}</span>
          <h2 className="mt-4 text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            {t(`${currentKey}Title`)}
          </h2>
          <p className="mt-2 text-sm text-zinc-500 dark:text-muted-foreground">
            {t(`${currentKey}Description`)}
          </p>
        </div>

        {/* Actions */}
        <div className="mt-8 flex items-center justify-between">
          <button
            onClick={dismiss}
            className="text-xs text-muted-foreground hover:text-zinc-600"
          >
            {t("skip")}
          </button>
          <Button
            variant="primary"
            size="sm"
            onClick={isLast ? dismiss : () => setStep((s) => s + 1)}
          >
            {isLast ? t("getStarted") : t("next")}
          </Button>
        </div>
      </div>
    </div>
  );
}

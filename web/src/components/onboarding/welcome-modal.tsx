"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import {
  Database01Icon,
  AiBrain01Icon,
  Analytics01Icon,
} from "@hugeicons/core-free-icons";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { getWorkspaceId } from "@/lib/workspace";
import { useAuth } from "@/hooks/use-auth";

/**
 * Per-workspace onboarding state. A user who joins a second workspace
 * sees the welcome flow once for that workspace; dismissing it in
 * workspace A doesn't silently swallow it for workspace B.
 */
function storageKey(workspaceId: string): string {
  return `ontosyx.onboarded.${workspaceId}`;
}

interface OnboardingStep {
  key: string;
  icon: IconSvgElement;
}

const STEPS: readonly OnboardingStep[] = [
  { key: "step1", icon: Database01Icon },
  { key: "step2", icon: AiBrain01Icon },
  { key: "step3", icon: Analytics01Icon },
] as const;

function subscribeToOnboardingStatus(onChange: () => void): () => void {
  window.addEventListener("storage", onChange);
  return () => window.removeEventListener("storage", onChange);
}

function makeStatusReader(workspaceId: string | undefined) {
  return () => {
    if (!workspaceId) return true;
    return !!localStorage.getItem(storageKey(workspaceId));
  };
}

/** SSR: assume already onboarded so the modal never flashes during SSR HTML. */
function getServerOnboardedStatus(): boolean {
  return true;
}

export function WelcomeModal() {
  const [step, setStep] = useState(0);
  const t = useTranslations("welcome");
  const workspaceId = getWorkspaceId();
  const { mode } = useAuth();
  const onboarded = useSyncExternalStore(
    subscribeToOnboardingStatus,
    makeStatusReader(workspaceId),
    getServerOnboardedStatus,
  );
  const dialogRef = useRef<HTMLDivElement>(null);
  // Only render once the principal is settled — `authenticated` for
  // multi-tenant deployments, `disabled` for single-tenant / dev. The
  // onboarding state is workspace-scoped, so we also gate on a
  // resolved workspace id.
  const sessionReady =
    mode.kind === "authenticated" || mode.kind === "disabled";
  const visible = sessionReady && !onboarded && !!workspaceId;

  // Move focus into the dialog on first mount so keyboard users land
  // on the primary CTA. Subsequent step changes don't steal focus —
  // a user navigating with Tab to the skip button shouldn't get
  // yanked back to "다음" on click.
  useEffect(() => {
    if (!visible) return;
    const cta = dialogRef.current?.querySelector<HTMLElement>(
      "[data-autofocus]",
    );
    cta?.focus();
  }, [visible]);

  if (!visible) return null;

  const key = storageKey(workspaceId);
  const dismiss = () => {
    localStorage.setItem(key, "true");
    window.dispatchEvent(new StorageEvent("storage", { key }));
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") dismiss();
  };

  const current = STEPS[step];
  const isLast = step === STEPS.length - 1;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-title"
      aria-describedby="welcome-description"
      onKeyDown={handleKeyDown}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
    >
      <div
        ref={dialogRef}
        className="w-full max-w-md rounded-2xl border border-divider bg-surface-base p-8 shadow-2xl"
      >
        {/* Step indicator */}
        <div className="mb-6 flex justify-center gap-2" aria-hidden>
          {STEPS.map((_, i) => (
            <div
              key={i}
              className={cn(
                "h-1.5 w-8 rounded-full transition-colors",
                i === step ? "bg-brand-solid" : "bg-surface-inset",
              )}
            />
          ))}
        </div>

        {/* Content */}
        <div className="text-center">
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-brand-surface">
            <HugeiconsIcon
              icon={current.icon}
              className="h-7 w-7 text-brand-foreground"
              size="100%"
            />
          </div>
          <h2
            id="welcome-title"
            className="mt-5 text-lg font-semibold text-foreground-strong"
          >
            {t(`${current.key}Title`)}
          </h2>
          <p
            id="welcome-description"
            className="mt-2 text-sm leading-relaxed text-foreground-muted"
          >
            {t(`${current.key}Description`)}
          </p>
        </div>

        {/* Actions */}
        <div className="mt-8 flex items-center justify-between">
          <button
            type="button"
            onClick={dismiss}
            className="rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-2"
          >
            {t("skip")}
          </button>
          <Button
            data-autofocus
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

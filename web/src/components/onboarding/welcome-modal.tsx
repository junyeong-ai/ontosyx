"use client";

import { useState, useSyncExternalStore } from "react";
import { useTranslations } from "next-intl";
import { Dialog } from "@base-ui/react/dialog";
import { AnimatePresence, motion } from "motion/react";
import type { LucideIcon } from "lucide-react";
import { BarChart3, Database } from "lucide-react";
import { Brain } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";
import { getWorkspaceId } from "@/lib/workspace";
import { useAuth } from "@/hooks/use-auth";
import { DynamicIcon } from "@/components/ui/dynamic-icon";

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
  icon: LucideIcon;
}

const STEPS: readonly OnboardingStep[] = [
  { key: "step1", icon: Database },
  { key: "step2", icon: Brain },
  { key: "step3", icon: BarChart3 },
] as const;

// Custom event name — *not* the native `"storage"` event. The native event
// is observed by third-party libs (e.g. Tanstack Query devtools) which
// proactively `removeItem` on unrecognised keys, so dispatching a synthetic
// `StorageEvent` to wake up our own listener also wipes our value mid-flight.
const ONBOARDING_CHANGE_EVENT = "ontosyx:onboarding-change";

function subscribeToOnboardingStatus(onChange: () => void): () => void {
  // Cross-tab sync still rides the real `storage` event (browsers fire it
  // in *other* tabs when localStorage mutates). Same-tab updates are
  // signalled through our private custom event.
  window.addEventListener("storage", onChange);
  window.addEventListener(ONBOARDING_CHANGE_EVENT, onChange);
  return () => {
    window.removeEventListener("storage", onChange);
    window.removeEventListener(ONBOARDING_CHANGE_EVENT, onChange);
  };
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

/**
 * Welcome modal — first-run onboarding flow per workspace.
 *
 * Built on Base UI Dialog so focus trap, Escape-to-close, scroll lock,
 * and trigger-focus restore come for free; only the step-swap animation
 * is hand-rolled (motion/react `AnimatePresence`). The dialog opens
 * automatically when `visible` flips to true and closes through the
 * single `onOpenChange` path — Skip button, Esc, outside click, and
 * the Get-started CTA all converge on `dismiss()`, which records the
 * per-workspace onboarded flag and broadcasts the same-tab event the
 * external store subscribes to.
 */
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
  // Only render once the principal is settled — `authenticated` for
  // multi-tenant deployments, `disabled` for single-tenant / dev. The
  // onboarding state is workspace-scoped, so we also gate on a
  // resolved workspace id.
  const sessionReady =
    mode.kind === "authenticated" || mode.kind === "disabled";
  const visible = sessionReady && !onboarded && !!workspaceId;

  const dismiss = () => {
    if (!workspaceId) return;
    localStorage.setItem(storageKey(workspaceId), "true");
    window.dispatchEvent(new Event(ONBOARDING_CHANGE_EVENT));
  };

  const current = STEPS[step];
  const isLast = step === STEPS.length - 1;

  return (
    <Dialog.Root
      open={visible}
      onOpenChange={(open) => {
        if (!open) dismiss();
      }}
    >
      <Dialog.Portal>
        <Dialog.Backdrop
          className={cn(
            "fixed inset-0 z-overlay bg-surface-scrim-strong backdrop-blur-sm",
            "transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)]",
            "data-[ending-style]:opacity-0 data-[starting-style]:opacity-0",
          )}
        />
        <Dialog.Popup
          className={cn(
            "fixed left-1/2 top-1/2 z-modal w-full max-w-md -translate-x-1/2 -translate-y-1/2",
            "rounded-2xl border border-divider bg-surface-base p-8 shadow-4",
            "transition-[opacity,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]",
            "data-[starting-style]:scale-[0.96] data-[starting-style]:opacity-0",
            "data-[ending-style]:scale-[0.96] data-[ending-style]:opacity-0",
          )}
        >
          {/* Step indicator */}
          <div className="mb-6 flex justify-center gap-2" aria-hidden="true">
            {STEPS.map((_, i) => (
              <div
                key={i}
                className={cn(
                  "h-1.5 w-8 rounded-full transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                  i === step ? "bg-brand-solid" : "bg-surface-inset",
                )}
              />
            ))}
          </div>

          {/* Step content — `AnimatePresence mode="wait"` swaps the
              icon + title + body together so the transition reads as a
              single slide. `Dialog.Title` / `Dialog.Description` are
              re-mounted per step; Base UI re-wires `aria-labelledby` /
              `aria-describedby` on the popup as the active descendants
              change, so screen readers always read the current step. */}
          <div className="relative text-center">
            <AnimatePresence mode="wait" initial={false}>
              <motion.div
                key={current.key}
                initial={{ opacity: 0, x: 12 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: -12 }}
                transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
              >
                <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-brand-surface">
                  <DynamicIcon as={current.icon} className="h-7 w-7 text-brand-foreground" />
                </div>
                <Dialog.Title className="mt-5 text-lg font-semibold text-foreground-strong">
                  {t(`${current.key}Title`)}
                </Dialog.Title>
                <Dialog.Description className="mt-2 text-sm leading-relaxed text-foreground-muted">
                  {t(`${current.key}Description`)}
                </Dialog.Description>
              </motion.div>
            </AnimatePresence>
          </div>

          {/* Actions */}
          <div className="mt-8 flex items-center justify-between">
            <Dialog.Close
              className="rounded-md px-2 py-1 text-xs text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-2"
            >
              {t("skip")}
            </Dialog.Close>
            <Button
              variant="primary"
              size="sm"
              onClick={isLast ? dismiss : () => setStep((s) => s + 1)}
            >
              {isLast ? t("getStarted") : t("next")}
            </Button>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

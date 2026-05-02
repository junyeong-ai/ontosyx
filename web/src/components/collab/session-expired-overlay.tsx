// Session-expired overlay — persistent re-auth prompt.
//
// Why a dedicated surface (not a toast): an expired session blocks
// every subsequent network call. A 5-second toast is the wrong
// affordance — the user needs a non-dismissible cue that survives
// route changes and explains *why* nothing is loading. Linear,
// Stripe, and Notion all surface auth-loss as a corner card with a
// dual-line message + bottom-right CTA. The toast queue stays free
// for transient warnings.
//
// Mounted once at the root layout. Listens to the collab store's
// `lastError` and renders only when classification reports a
// re-auth code (single source of truth in `error-classification.ts`).

"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Alert02Icon, Cancel01Icon } from "@hugeicons/core-free-icons";

import { Button } from "@/components/ui/button";
import {
  isReauthCode,
  selectLastError,
  useCollabStore,
} from "@/lib/collab";
import { useAuth } from "@/hooks/use-auth";

export function SessionExpiredOverlay() {
  const lastError = useCollabStore(selectLastError);
  const { authEnabled } = useAuth();
  const t = useTranslations("collaboration.errors");
  const tActions = useTranslations("collaboration.actions");
  const router = useRouter();
  const ctaRef = useRef<HTMLButtonElement>(null);
  // Per-error-instance dismissal — clearing on every new error code
  // so a fresh re-auth event after a manual dismiss reappears.
  const [dismissedCode, setDismissedCode] = useState<string | null>(null);

  const errorCode = lastError?.code;
  useEffect(() => {
    if (errorCode && errorCode !== dismissedCode && authEnabled) {
      ctaRef.current?.focus();
    }
  }, [errorCode, dismissedCode, authEnabled]);

  // When auth is disabled (single-tenant dev / on-prem) there is no
  // "sign in again" action to surface — collab WS errors in that mode
  // are recoverable churn (server restart, network blip), not session
  // loss. Skip the overlay entirely so the dev shell isn't peppered
  // with a permanent CTA every time the WS reconnects.
  if (!authEnabled) return null;
  if (!errorCode || !isReauthCode(errorCode)) return null;
  if (errorCode === dismissedCode) return null;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") setDismissedCode(errorCode);
  };

  return (
    <div
      role="alertdialog"
      aria-labelledby="session-expired-title"
      aria-describedby="session-expired-description"
      onKeyDown={handleKeyDown}
      className="fixed bottom-6 right-6 z-50 w-[22rem] overflow-hidden rounded-xl border border-divider bg-surface-base shadow-2xl"
    >
      <div className="flex gap-3 px-5 pt-5">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-warning-surface">
          <HugeiconsIcon
            icon={Alert02Icon}
            className="h-5 w-5 text-warning-foreground"
            size="100%"
          />
        </div>
        <div className="min-w-0 flex-1">
          <p
            id="session-expired-title"
            className="text-sm font-semibold text-foreground-strong"
          >
            {t(`${errorCode}.title`)}
          </p>
          <p
            id="session-expired-description"
            className="mt-1 text-xs leading-relaxed text-foreground-muted"
          >
            {t(`${errorCode}.description`)}
          </p>
        </div>
        <button
          type="button"
          onClick={() => setDismissedCode(errorCode)}
          aria-label={tActions("dismiss")}
          className="-mr-2 -mt-2 flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-inset hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground"
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-3.5 w-3.5" size="100%" />
        </button>
      </div>
      <div className="flex items-center justify-end gap-2 px-5 py-4">
        <Button
          ref={ctaRef}
          variant="primary"
          size="sm"
          onClick={() => router.push("/login")}
        >
          {tActions("signInAgain")}
        </Button>
      </div>
    </div>
  );
}

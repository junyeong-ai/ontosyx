// CollaborationErrorToaster — translates `ServerMessage::Error`
// frames into localised toasts. Mounted once at the workbench
// shell; the i18n catalogue carries one message per `ErrorCode`.

"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { selectLastError, useCollabStore } from "@/lib/collab";

/**
 * Background warnings — surface as transient toasts and let the
 * user keep working. Everything else is treated as fatal.
 */
const TRANSIENT_CODES: ReadonlySet<string> = new Set([
  "broadcast_lagged",
  "not_joined",
]);

/**
 * Codes for which the only meaningful action is re-authentication.
 * The toast carries an explicit "Sign in again" button so the user
 * isn't stranded — clicking routes to the login flow rather than
 * waiting for them to figure out where the session bounced from.
 */
const REAUTH_CODES: ReadonlySet<string> = new Set([
  "auth_required",
  "auth_invalid",
  "auth_timeout",
  "session_revoked",
  "unauthorized_workspace",
]);

export function CollaborationErrorToaster() {
  const lastError = useCollabStore(selectLastError);
  const t = useTranslations("collaboration.errors");
  const tActions = useTranslations("collaboration.actions");
  const router = useRouter();
  const lastSeenRef = useRef<{ code: string; ts: number } | null>(null);

  useEffect(() => {
    if (!lastError) return;
    // De-dupe back-to-back identical codes (e.g. broadcast_lagged
    // bursts on a slow client) within a 2-second window.
    const now = Date.now();
    const seen = lastSeenRef.current;
    if (seen && seen.code === lastError.code && now - seen.ts < 2_000) {
      return;
    }
    lastSeenRef.current = { code: lastError.code, ts: now };

    const message = safeTranslate(t, lastError.code);
    if (TRANSIENT_CODES.has(lastError.code)) {
      toast.warning(message);
      return;
    }
    if (REAUTH_CODES.has(lastError.code)) {
      toast.error(message, {
        action: {
          label: tActions("signInAgain"),
          onClick: () => router.push("/login"),
        },
        duration: Infinity,
      });
      return;
    }
    toast.error(message);
  }, [lastError, t, tActions, router]);

  return null;
}

/**
 * `next-intl` throws when a key is missing. Server-side enums can
 * outpace the catalogue between releases, so we render the raw
 * `code` as a last-resort placeholder rather than crashing the
 * tree.
 */
function safeTranslate(
  t: (key: string) => string,
  code: string,
): string {
  try {
    return t(code);
  } catch {
    return code;
  }
}

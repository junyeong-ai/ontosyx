// CollaborationErrorToaster — translates `ServerMessage::Error`
// frames into localised toasts. Mounted once at the workbench
// shell; the i18n catalogue carries one message per `ErrorCode`.

"use client";

import { useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { selectLastError, useCollabStore } from "@/lib/collab";

/**
 * Dismissable, non-fatal codes that surface as background warnings.
 * Fatal codes (auth_required, auth_invalid, session_revoked, etc.)
 * usually arrive right before the socket closes — the reconnect
 * banner / sign-in flow handles them; we still show a toast so the
 * user understands why they bounced.
 */
const TRANSIENT_CODES: ReadonlySet<string> = new Set([
  "broadcast_lagged",
  "not_joined",
]);

export function CollaborationErrorToaster() {
  const lastError = useCollabStore(selectLastError);
  const t = useTranslations("collaboration.errors");
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
    } else {
      toast.error(message);
    }
  }, [lastError, t]);

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

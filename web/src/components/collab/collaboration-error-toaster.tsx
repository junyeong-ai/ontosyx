// CollaborationErrorToaster — translates `ServerMessage::Error`
// frames into localised toasts. Mounted once at the workbench
// shell.
//
// Re-auth codes are owned by `<SessionExpiredOverlay>` — a
// persistent corner card with a dedicated CTA. The classification
// table in `error-classification.ts` is the single source of truth
// for which surface handles which code.

"use client";

import { useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import {
  classifyError,
  clearWsTokenCache,
  selectLastError,
  useCollabStore,
} from "@/lib/collab";

/** Identical-code de-dupe window. Slow clients can fan-out the same
 *  warning multiple times within a single tick — this collapses the
 *  burst to one visible toast. */
const DEDUPE_WINDOW_MS = 2_000;

export function CollaborationErrorToaster() {
  const lastError = useCollabStore(selectLastError);
  const t = useTranslations("collaboration.errors");
  const lastSeenRef = useRef<{ code: string; ts: number } | null>(null);

  useEffect(() => {
    if (!lastError) return;
    const now = Date.now();
    const seen = lastSeenRef.current;
    if (seen && seen.code === lastError.code && now - seen.ts < DEDUPE_WINDOW_MS) {
      return;
    }
    lastSeenRef.current = { code: lastError.code, ts: now };

    const surface = classifyError(lastError.code);
    if (surface === "reauth") {
      // The cached WS token is the one the server just rejected.
      // Drop it so the next reconnect mints a fresh JWT instead of
      // replaying the revoked one. The overlay owns the user-facing
      // surface from here.
      clearWsTokenCache();
      return;
    }

    const title = t(`${lastError.code}.title`);
    const description = t(`${lastError.code}.description`);
    const opts = { description };
    if (surface === "transient") {
      toast.warning(title, opts);
      return;
    }
    toast.error(title, opts);
  }, [lastError, t]);

  return null;
}

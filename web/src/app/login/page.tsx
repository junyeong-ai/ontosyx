"use client";

import { useSearchParams, useRouter } from "next/navigation";
import { Suspense, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useQueryClient } from "@tanstack/react-query";

import { useAuth, authKeys } from "@/hooks/use-auth";
import { Spinner } from "@/components/ui/spinner";

const KNOWN_LOGIN_ERRORS = ["token_exchange_failed", "not_configured"] as const;
type KnownLoginError = (typeof KNOWN_LOGIN_ERRORS)[number];

function isKnownLoginError(value: string): value is KnownLoginError {
  return (KNOWN_LOGIN_ERRORS as readonly string[]).includes(value);
}

/** Internal-only safe redirect target. Strict same-origin path so the
 *  `next` param can't be used to bounce a user out to a phishing site. */
function safeNext(value: string | null): string {
  if (!value) return "/design";
  if (!value.startsWith("/")) return "/design";
  if (value.startsWith("//")) return "/design";
  return value;
}

function LoginContent() {
  const t = useTranslations("page.login");
  const searchParams = useSearchParams();
  const router = useRouter();
  const qc = useQueryClient();
  const { mode } = useAuth();
  const [signingIn, setSigningIn] = useState(false);
  const error = searchParams.get("error");
  const next = safeNext(searchParams.get("next"));

  // Logout flow lands here via server redirect; the cookie has been
  // cleared but TanStack Query still holds the previous /auth/me
  // payload. Invalidate on mount so the form below renders against a
  // fresh fetch instead of flashing the bookmark-style auto-redirect
  // for the just-signed-out user.
  useEffect(() => {
    qc.invalidateQueries({ queryKey: authKeys.me() });
  }, [qc.invalidateQueries]);

  // Already-authenticated users hitting `/login` directly (bookmark,
  // double-tap nav) land on their workbench, not a sign-in button.
  // Auth-disabled deployments skip the page entirely — there's no
  // sign-in to perform when the backend isn't gating access.
  useEffect(() => {
    if (mode.kind === "authenticated" || mode.kind === "disabled") {
      router.replace(next);
    }
  }, [mode.kind, next, router]);

  const googleHref = `/auth/google?next=${encodeURIComponent(next)}`;

  const resolvedError = (() => {
    if (!error) return null;
    if (isKnownLoginError(error)) {
      return error === "token_exchange_failed"
        ? t("errorTokenExchange")
        : t("errorNotConfigured");
    }
    return t("errorGeneric", { error });
  })();

  // Single `<main id="main">` regardless of auth state — the skip-link
  // target stays stable, and React's reconciler doesn't unmount-mount
  // the landmark when the round-trip resolves. Body content swaps
  // between the resolving spinner and the sign-in card.
  const isResolving = mode.kind !== "unauthenticated";

  return (
    <main
      id="main"
      tabIndex={0}
      className="flex min-h-screen items-center justify-center bg-surface-raised px-4 outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
    >
      {isResolving ? (
        <Spinner size="lg" className="text-brand-foreground" />
      ) : (
        <div className="w-full max-w-sm">
          <div className="rounded-2xl border border-divider bg-surface-base p-8 shadow-4">
            <div className="text-center">
              <h1 className="text-2xl font-semibold tracking-tight text-foreground-strong">
                {t("appTitle")}
              </h1>
              <p className="mt-2 text-sm text-foreground-muted">{t("tagline")}</p>
            </div>

            {resolvedError && (
              <div
                role="alert"
                className="mt-6 rounded-lg border border-danger-border bg-danger-surface px-4 py-3 text-sm text-danger-foreground"
              >
                {resolvedError}
              </div>
            )}

            <a
              href={googleHref}
              onClick={() => setSigningIn(true)}
              aria-disabled={signingIn || undefined}
              className="mt-6 flex items-center justify-center gap-3 rounded-lg border border-divider bg-surface-base px-6 py-3 text-sm font-medium text-foreground-strong transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-2 aria-disabled:pointer-events-none aria-disabled:opacity-60"
            >
              {signingIn ? (
                <Spinner size="sm" />
              ) : (
                <svg className="h-5 w-5" viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"
                    fill="#4285F4"
                  />
                  <path
                    d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
                    fill="#34A853"
                  />
                  <path
                    d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
                    fill="#FBBC05"
                  />
                  <path
                    d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
                    fill="#EA4335"
                  />
                </svg>
              )}
              {signingIn ? t("signingIn") : t("signInGoogle")}
            </a>
          </div>
        </div>
      )}
    </main>
  );
}

export default function LoginPage() {
  return (
    <Suspense>
      <LoginContent />
    </Suspense>
  );
}

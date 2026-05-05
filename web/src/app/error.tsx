"use client";

/**
 * Root error boundary for the App Router.
 *
 * Triggers on any uncaught error inside a server/client component during
 * rendering of the tree below `layout.tsx`. Must be a Client Component.
 */

import { useEffect } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import { AlertCircle, Home, RefreshCw } from "lucide-react";
import { Button, buttonStyles } from "@/components/ui/button";

interface RootErrorProps {
  error: Error & { digest?: string };
  reset: () => void;
}

export default function RootError({ error, reset }: RootErrorProps) {
  const t = useTranslations("page.error");

  useEffect(() => {
    // Log to console for operators. In production this should also flush to
    // a telemetry endpoint — left as a follow-up since we don't have a
    // shared client error reporter yet.
    console.error("[RootError]", error);
  }, [error]);

  const subject = error.digest
    ? t("reportSubjectWithDigest", { digest: error.digest })
    : t("reportSubject");

  return (
    <html lang="ko">
      <body className="flex min-h-dvh items-center justify-center bg-surface-raised">
        <main id="main" className="mx-4 w-full max-w-md rounded-xl border border-divider bg-surface-base p-6 shadow-1">
          <div className="flex items-start gap-3">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-danger-surface/30">
              <AlertCircle className="h-5 w-5 text-danger-foreground" />
            </div>
            <div className="min-w-0 flex-1">
              <h1 className="text-base font-semibold text-foreground-strong">
                {t("title")}
              </h1>
              <p className="mt-1 text-sm text-foreground">
                {t("description")}
              </p>
              <p className="mt-1 text-xs text-foreground-muted">
                {t("subtitle")}
              </p>

              {error.digest && (
                <p className="mt-3 rounded bg-surface-raised px-2 py-1 font-mono text-2xs text-foreground-muted">
                  {t("refPrefix")} {error.digest}
                </p>
              )}

              <div className="mt-5 flex flex-wrap gap-2">
                <Button
                  onClick={reset}
                  variant="primary"
                  size="sm"
                  leadingIcon={
                    <RefreshCw className="h-3.5 w-3.5" />
                  }
                >
                  {t("tryAgain")}
                </Button>
                <Link href="/" className={buttonStyles({ variant: "outline", size: "sm" })}>
                  <Home className="h-3.5 w-3.5" />
                  {t("home")}
                </Link>
                <a
                  href={`mailto:support@ontosyx.io?subject=${encodeURIComponent(
                    subject,
                  )}&body=${encodeURIComponent(
                    `Error message: ${error.message}\nDigest: ${error.digest ?? "n/a"}\nURL: ${typeof window !== "undefined" ? window.location.href : ""}\n`,
                  )}`}
                  className={buttonStyles({ variant: "ghost", size: "sm" })}
                >
                  {t("reportError")}
                </a>
              </div>
            </div>
          </div>
        </main>
      </body>
    </html>
  );
}

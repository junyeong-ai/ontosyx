"use client";

/**
 * Root error boundary for the App Router.
 *
 * Triggers on any uncaught error inside a server/client component during
 * rendering of the tree below `layout.tsx`. Must be a Client Component.
 *
 * Copy is Korean-first because the product's primary user base is Korean.
 * English is shown underneath as a tertiary line for international users.
 */

import { useEffect } from "react";
import Link from "next/link";
import { HugeiconsIcon } from "@hugeicons/react";
import { AlertCircleIcon, Home01Icon, RefreshIcon } from "@hugeicons/core-free-icons";

interface RootErrorProps {
  error: Error & { digest?: string };
  reset: () => void;
}

export default function RootError({ error, reset }: RootErrorProps) {
  useEffect(() => {
    // Log to console for operators. In production this should also flush to
    // a telemetry endpoint — left as a follow-up since we don't have a
    // shared client error reporter yet.
    console.error("[RootError]", error);
  }, [error]);

  return (
    <html lang="ko">
      <body className="flex min-h-dvh items-center justify-center bg-zinc-50 dark:bg-zinc-950">
        <div className="mx-4 w-full max-w-md rounded-xl border border-zinc-200 bg-white p-6 shadow-sm dark:border-zinc-800 dark:bg-zinc-900">
          <div className="flex items-start gap-3">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-red-50 dark:bg-red-950/30">
              <HugeiconsIcon
                icon={AlertCircleIcon}
                className="h-5 w-5 text-red-500"
                size="100%"
              />
            </div>
            <div className="min-w-0 flex-1">
              <h1 className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
                문제가 발생했어요
              </h1>
              <p className="mt-1 text-sm text-zinc-600 dark:text-zinc-400">
                페이지를 불러오는 중 오류가 발생했습니다. 잠시 후 다시
                시도해 주세요.
              </p>
              <p className="mt-1 text-xs text-zinc-400">
                Something went wrong while rendering the page.
              </p>

              {error.digest && (
                <p className="mt-3 rounded bg-zinc-50 px-2 py-1 font-mono text-[10px] text-zinc-500 dark:bg-zinc-950 dark:text-zinc-400">
                  ref: {error.digest}
                </p>
              )}

              <div className="mt-5 flex flex-wrap gap-2">
                <button
                  onClick={reset}
                  className="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-emerald-700"
                >
                  <HugeiconsIcon
                    icon={RefreshIcon}
                    className="h-3.5 w-3.5"
                    size="100%"
                  />
                  다시 시도 (Try again)
                </button>
                <Link
                  href="/"
                  className="inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
                >
                  <HugeiconsIcon
                    icon={Home01Icon}
                    className="h-3.5 w-3.5"
                    size="100%"
                  />
                  홈으로 (Home)
                </Link>
                <a
                  href={`mailto:support@ontosyx.io?subject=${encodeURIComponent(
                    `[Ontosyx] Error report${error.digest ? `: ${error.digest}` : ""}`,
                  )}&body=${encodeURIComponent(
                    `Error message: ${error.message}\nDigest: ${error.digest ?? "n/a"}\nURL: ${typeof window !== "undefined" ? window.location.href : ""}\n`,
                  )}`}
                  className="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium text-zinc-500 transition-colors hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200"
                >
                  오류 신고 (Report)
                </a>
              </div>
            </div>
          </div>
        </div>
      </body>
    </html>
  );
}

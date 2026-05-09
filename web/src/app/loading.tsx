"use client";

/**
 * Root loading UI for the App Router.
 *
 * Shown during server-side navigation/streaming while a route segment is
 * being rendered. Matches the sidebar + header shell of `page.tsx` and
 * paints data-shaped skeletons in the main area so the UI doesn't jump
 * when the real content swaps in. Skeleton-with-pulse over a centred
 * spinner — the data slots tell the operator "list-shaped content is
 * loading here" before any byte arrives, which a spinner cannot.
 *
 * Client component for the same reason `not-found.tsx` is — async
 * server components participate in Next.js' dev-mode performance
 * instrumentation (`Performance.measure '<ComponentName>'`), and a
 * loading boundary fires often enough that the dev console fills with
 * those marks. The client renderer skips that path; the served HTML
 * is identical.
 */

import { useTranslations } from "next-intl";
import { SkeletonList } from "@/components/ui/skeleton";

export default function RootLoading() {
  const t = useTranslations("loading");

  return (
    <div
      className="flex h-dvh overflow-hidden bg-surface-raised"
      role="status"
      aria-live="polite"
      aria-label={t("messageAria")}
    >
      <div className="w-rail shrink-0 border-e border-divider" aria-hidden />

      <div className="flex flex-1 flex-col overflow-hidden">
        <div className="h-10 shrink-0 border-b border-divider" aria-hidden />

        <main
          id="main"
          tabIndex={0}
          className="relative flex-1 overflow-hidden outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
        >
          <div className="mx-auto max-w-3xl px-6 py-8">
            <SkeletonList count={6} />
          </div>
        </main>
      </div>
    </div>
  );
}

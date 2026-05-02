/**
 * Root loading UI for the App Router.
 *
 * Shown during server-side navigation/streaming while a route segment is
 * being rendered. Matches the sidebar + header shell of `page.tsx` so the
 * UI doesn't jump when the real content swaps in.
 */

import { getTranslations } from "next-intl/server";
import { Spinner } from "@/components/ui/spinner";

export default async function RootLoading() {
  const t = await getTranslations("loading");

  return (
    <div className="flex h-dvh overflow-hidden bg-surface-raised">
      {/* Sidebar skeleton */}
      <div className="w-12 shrink-0 border-r border-divider" />

      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Header skeleton */}
        <div className="h-10 shrink-0 border-b border-divider" />

        {/* Content skeleton + spinner */}
        <main id="main" className="relative flex-1 overflow-hidden">
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3">
            <Spinner size="md" className="text-brand-foreground" />
            <p className="text-xs text-muted-foreground" aria-label={t("messageAria")}>
              {t("message")}
            </p>
          </div>
        </main>
      </div>
    </div>
  );
}

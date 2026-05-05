"use client";

/**
 * Root 404 page for the App Router.
 *
 * Rendered when Next.js cannot match a route, or when a component calls
 * `notFound()`. The page is a client component because:
 *
 *   1. Its content is fully static — no server-side data fetch — so
 *      shipping it as a server component would gain nothing.
 *   2. Async server components participate in Next.js' dev-mode
 *      performance instrumentation; the marker name leaks through to
 *      the console as "Performance.measure 'NotFound'" with a
 *      negative timestamp when the route is hit mid-navigation. The
 *      client-renderer path doesn't emit that mark.
 *
 * `useTranslations` is the client-side counterpart to the server
 * `getTranslations` — same locale chain, same message bundle.
 */

import Link from "next/link";
import { useTranslations } from "next-intl";
import { HelpCircle, Home, Search } from "lucide-react";
import { buttonStyles } from "@/components/ui/button";
import { Heading } from "@/components/ui/heading";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";

export default function NotFound() {
  const t = useTranslations("notFound");

  return (
    <main
      id="main"
      tabIndex={0}
      className="flex min-h-dvh items-center justify-center bg-surface-raised px-4 outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
    >
      <div className="w-full max-w-md text-center">
        <p className="text-xs font-semibold uppercase tracking-widest text-brand-foreground">
          {t("code")}
        </p>
        <Heading level={1} size={2} className="mt-2">
          {t("title")}
        </Heading>
        <p className="mt-2 text-sm text-foreground">
          {t("description")}
        </p>
        <p className="mt-1 text-xs text-foreground-muted">{t("hint")}</p>

        <div className="mt-8 flex flex-wrap items-center justify-center gap-2">
          <Link
            href="/"
            aria-label={t("homeAria")}
            className={buttonStyles({ variant: "primary", size: "md" })}
          >
            <Home className="h-3.5 w-3.5" />
            {t("home")}
          </Link>
          <Link
            href="/?onboarding=1"
            aria-label={t("getStartedAria")}
            className={buttonStyles({ variant: "outline", size: "md" })}
          >
            <HelpCircle className="h-3.5 w-3.5" />
            {t("getStarted")}
          </Link>
        </div>

        <p className="mt-8 flex items-center justify-center gap-1.5 text-2xs text-foreground-muted">
          <Search className="h-3 w-3" />
          <span>
            {t("searchTipPrefix")}
            <KeyboardShortcut keys="mod+k" variant="outline" className="mx-1" />
            {t("searchTipSuffix")}
          </span>
        </p>
      </div>
    </main>
  );
}

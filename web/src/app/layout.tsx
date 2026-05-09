import type { Metadata } from "next";
import { Geist, Geist_Mono, Noto_Sans_KR } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages, getTranslations } from "next-intl/server";
import { Toaster } from "@/components/ui/toast";
import { ConfirmProvider } from "@/components/providers/confirm-provider";
import { PromptProvider } from "@/components/providers/prompt-provider";
import { I18nBridgeProvider } from "@/lib/i18n-bridge";
import { directionForLocale } from "@/lib/locale/dir";
import { NarrowViewportBanner } from "@/components/ui/narrow-viewport-banner";
import { QueryProvider } from "@/components/providers/query-provider";
import { A11yProvider } from "@/components/providers/a11y-provider";
import { ExtensionsBoot } from "@/extensions/extensions-boot";
import { ShortcutDispatcher } from "@/lib/shortcuts";
import { CommandPaletteHost } from "@/components/layout/command-palette-host";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

// Noto Sans KR — Google-hosted Korean typeface. Paired with Pretendard
// (CDN preloaded in <head> below) as the primary stack, with Geist /
// system-ui as Latin fallbacks.
const notoSansKr = Noto_Sans_KR({
  variable: "--font-noto-sans-kr",
  subsets: ["latin"],
  weight: ["400", "500", "600", "700"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Ontosyx",
  description: "Knowledge Graph Lifecycle Platform",
};

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  // Locale is resolved server-side from the `ontosyx_locale` cookie
  // (see `src/i18n/request.ts`). `getMessages()` then serialises the
  // active locale's JSON bundle into the client provider below, so
  // every descendant — RSC or client — reads from the same snapshot.
  const locale = await getLocale();
  const messages = await getMessages();
  const tSkip = await getTranslations("chrome.skipLinks");
  // `dir` is derived from the active locale so every CSS rule that
  // uses logical properties (`start` / `end`) flips automatically
  // when an RTL bundle lands, no per-component changes required.
  // Today's `en` + `ko` both resolve to `ltr`; future locales pick
  // up RTL through `directionForLocale`.
  const dir = directionForLocale(locale);

  return (
    <html
      lang={locale}
      dir={dir}
      className={`${geistSans.variable} ${geistMono.variable} ${notoSansKr.variable}`}
    >
      <head>
        {/* Pretendard via CDN — primary Korean typeface. Preload +
            stylesheet so CJK-heavy pages avoid FOUT. Variable weight. */}
        <link
          rel="preload"
          as="style"
          href="https://cdn.jsdelivr.net/gh/orioncactus/pretendard@v1.3.9/dist/web/variable/pretendardvariable.min.css"
        />
        <link
          rel="stylesheet"
          href="https://cdn.jsdelivr.net/gh/orioncactus/pretendard@v1.3.9/dist/web/variable/pretendardvariable.min.css"
        />
      </head>
      <body className="antialiased">
        {/* Skip links sit inside a `<nav>` landmark so they are
            contained by a region — bare body-level interactives trip
            axe's `region` rule. The chord (Tab on first focus)
            walks every landmark in document order, so a keyboard
            user can jump to whichever surface they care about
            without tabbing through the chrome. */}
        {/* Universal skip link — every page renders `<main id="main">`,
            so this target always resolves. Surface-specific
            destinations (`#sidebar`, `#inspector`) are added by the
            owning layout so axe doesn't flag a missing target on routes
            where they don't exist (e.g. login / not-found / loading
            have no sidebar at all). */}
        <nav aria-label={tSkip("labelGlobal")} className="contents">
          <a
            href="#main"
            className="sr-only focus:not-sr-only focus:absolute focus:start-4 focus:top-4 focus:z-skip-link focus:rounded-md focus:bg-brand-solid focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-foreground-onbrand focus:outline-none focus:ring-2 focus:ring-brand-foreground/40 focus:ring-offset-2"
          >
            {tSkip("toMain")}
          </a>
        </nav>
        <NextIntlClientProvider locale={locale} messages={messages}>
          <QueryProvider>
            <I18nBridgeProvider>
              <ConfirmProvider>
                <PromptProvider>
                  <ExtensionsBoot />
                  <ShortcutDispatcher />
                  <CommandPaletteHost />
                  {children}
                  <NarrowViewportBanner />
                </PromptProvider>
              </ConfirmProvider>
            </I18nBridgeProvider>
          </QueryProvider>
          {/* Dev-only axe-core runtime — tree-shaken in production. */}
          {process.env.NODE_ENV === "development" && <A11yProvider />}
          <div id="modal-root" />
          <Toaster />
        </NextIntlClientProvider>
      </body>
    </html>
  );
}

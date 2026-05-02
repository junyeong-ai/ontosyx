import type { Metadata } from "next";
import { Geist, Geist_Mono, Noto_Sans_KR } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages } from "next-intl/server";
import { Toaster } from "sonner";
import { ConfirmProvider } from "@/components/providers/confirm-provider";
import { WelcomeModal } from "@/components/onboarding/welcome-modal";
import { NarrowViewportBanner } from "@/components/ui/narrow-viewport-banner";
import { SessionExpiredOverlay } from "@/components/collab/session-expired-overlay";
import { QueryProvider } from "@/components/providers/query-provider";
import { A11yProvider } from "@/components/providers/a11y-provider";
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

  return (
    <html
      lang={locale}
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
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-50 focus:rounded-md focus:bg-brand-solid focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-foreground-onbrand focus:outline-none focus:ring-2 focus:ring-brand-foreground/40 focus:ring-offset-2"
        >
          본문으로 건너뛰기
        </a>
        <NextIntlClientProvider locale={locale} messages={messages}>
          <QueryProvider>
            <ConfirmProvider>{children}</ConfirmProvider>
            <WelcomeModal />
            <SessionExpiredOverlay />
            <NarrowViewportBanner />
          </QueryProvider>
          {/* Dev-only axe-core runtime — tree-shaken in production. */}
          {process.env.NODE_ENV === "development" && <A11yProvider />}
          <div id="modal-root" />
          <Toaster
            position="bottom-right"
            toastOptions={{
              className: "text-sm",
            }}
          />
        </NextIntlClientProvider>
      </body>
    </html>
  );
}

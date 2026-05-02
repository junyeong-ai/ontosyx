import type { Metadata } from "next";
import { Geist, Geist_Mono, Noto_Sans_KR } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages } from "next-intl/server";
import { Toaster } from "sonner";
import { ConfirmProvider } from "@/components/ui/confirm-dialog";
import { WelcomeModal } from "@/components/onboarding/welcome-modal";
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
    // next/font variable classes live on <html> so the runtime
    // CSS variables (`--font-geist-sans`, `--font-noto-sans-kr`,
    // `--font-geist-mono`) are in `:root` scope. globals.css's
    // `@theme inline` defines `--font-sans` on `:root, :host`
    // referencing those variables; if they aren't defined at the
    // same scope CSS treats the declaration as "invalid at computed
    // value time" and the whole font stack falls back to the UA
    // serif default. This is the official Next.js + Tailwind v4
    // pattern (see Tailwind docs: "Referencing other variables").
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
        {/* Skip-to-main — hidden until focused, lets keyboard users
            jump past the shell chrome straight to the content. */}
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-50 focus:rounded-md focus:bg-emerald-600 focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-white focus:outline-none focus:ring-2 focus:ring-emerald-400 focus:ring-offset-2"
        >
          본문으로 건너뛰기
        </a>
        <NextIntlClientProvider locale={locale} messages={messages}>
          <QueryProvider>
            <ConfirmProvider>
              <div id="main">{children}</div>
            </ConfirmProvider>
            <WelcomeModal />
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

import type { Metadata } from "next";
import { Geist, Geist_Mono, Noto_Sans_KR } from "next/font/google";
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

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="ko">
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
      <body
        className={`${geistSans.variable} ${geistMono.variable} ${notoSansKr.variable} antialiased`}
      >
        <QueryProvider>
          <ConfirmProvider>
            {children}
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
      </body>
    </html>
  );
}

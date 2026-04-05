import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { Toaster } from "sonner";
import { ConfirmProvider } from "@/components/ui/confirm-dialog";
import { WelcomeModal } from "@/components/onboarding/welcome-modal";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
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
    <html lang="en">
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <ConfirmProvider>
          {children}
        </ConfirmProvider>
        <WelcomeModal />
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

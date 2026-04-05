"use client";

import { useState, useEffect } from "react";
import { usePathname } from "next/navigation";
import { SettingsSidebar } from "@/components/settings/settings-sidebar";
import { WIDE_SETTINGS_PAGES } from "@/lib/constants/settings";
import { cn } from "@/lib/cn";

export default function SettingsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pathname = usePathname();
  const isWide = WIDE_SETTINGS_PAGES.has(pathname);

  // Prevent hydration mismatch: client-only state (useAuth, localStorage)
  // causes SSR/client tree divergence. Defer rendering until mounted.
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  return (
    <div className="flex h-screen bg-zinc-50 dark:bg-zinc-950">
      <SettingsSidebar />
      <main className="flex-1 overflow-y-auto p-6 lg:p-8">
        <div className={cn("mx-auto", isWide ? "max-w-6xl" : "max-w-3xl")}>
          {mounted ? children : null}
        </div>
      </main>
    </div>
  );
}

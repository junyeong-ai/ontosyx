"use client";

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

  return (
    <div className="flex h-screen bg-zinc-50 dark:bg-zinc-950">
      <SettingsSidebar />
      <main className="flex-1 overflow-y-auto p-6 lg:p-8">
        <div className={cn("mx-auto", isWide ? "max-w-6xl" : "max-w-3xl")}>
          {children}
        </div>
      </main>
    </div>
  );
}

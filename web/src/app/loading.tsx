/**
 * Root loading UI for the App Router.
 *
 * Shown during server-side navigation/streaming while a route segment is
 * being rendered. Matches the sidebar + header shell of `page.tsx` so the
 * UI doesn't jump when the real content swaps in.
 */

import { Spinner } from "@/components/ui/spinner";

export default function RootLoading() {
  return (
    <div className="flex h-dvh overflow-hidden bg-zinc-50 dark:bg-zinc-950">
      {/* Sidebar skeleton */}
      <div className="w-12 shrink-0 border-r border-zinc-200 dark:border-zinc-800" />

      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Header skeleton */}
        <div className="h-10 shrink-0 border-b border-zinc-200 dark:border-zinc-800" />

        {/* Content skeleton + spinner */}
        <main className="relative flex-1 overflow-hidden">
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3">
            <Spinner size="md" className="text-emerald-500" />
            <p className="text-xs text-zinc-400">불러오는 중… (Loading)</p>
          </div>
        </main>
      </div>
    </div>
  );
}

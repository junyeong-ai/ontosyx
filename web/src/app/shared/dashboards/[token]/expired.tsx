/**
 * Rendered when the share token returns HTTP 410 Gone — the link was once
 * valid but has since been expired or revoked (Phase 4.10).
 *
 * Exported both as the default component (for embedding from `page.tsx`)
 * and as-is under this path so it can be referenced directly.
 */

import Link from "next/link";
import { HugeiconsIcon } from "@hugeicons/react";
import { Link04Icon, Home01Icon } from "@hugeicons/core-free-icons";

export default function Expired() {
  return (
    <div className="flex min-h-dvh items-center justify-center bg-zinc-50 px-4 dark:bg-zinc-950">
      <div className="w-full max-w-md rounded-xl border border-zinc-200 bg-white p-6 text-center shadow-sm dark:border-zinc-800 dark:bg-zinc-900">
        <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-amber-50 dark:bg-amber-950/30">
          <HugeiconsIcon
            icon={Link04Icon}
            className="h-5 w-5 text-amber-500"
            size="100%"
          />
        </div>
        <h1 className="mt-4 text-base font-semibold text-zinc-900 dark:text-zinc-100">
          만료된 공유 링크예요
        </h1>
        <p className="mt-1 text-xs text-zinc-400">
          This share link has expired.
        </p>

        <p className="mt-4 text-sm text-zinc-600 dark:text-zinc-400">
          대시보드 소유자가 공유를 해제했거나, 링크의 유효 기간이 지났습니다.
          새로운 링크를 요청해 주세요.
        </p>

        <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
          <Link
            href="/"
            className="inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-4 py-2 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon
              icon={Home01Icon}
              className="h-3.5 w-3.5"
              size="100%"
            />
            홈으로 (Home)
          </Link>
        </div>
      </div>
    </div>
  );
}

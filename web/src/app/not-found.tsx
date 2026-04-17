/**
 * Root 404 page for the App Router.
 *
 * Rendered when Next.js cannot match a route, or when a component calls
 * `notFound()`. Korean copy first, English underneath.
 */

import Link from "next/link";
import { HugeiconsIcon } from "@hugeicons/react";
import { Home01Icon, HelpCircleIcon, Search01Icon } from "@hugeicons/core-free-icons";

export default function NotFound() {
  return (
    <div className="flex min-h-dvh items-center justify-center bg-zinc-50 px-4 dark:bg-zinc-950">
      <div className="w-full max-w-md text-center">
        <p className="text-xs font-semibold uppercase tracking-widest text-emerald-500">
          404
        </p>
        <h1 className="mt-2 text-2xl font-semibold text-zinc-900 dark:text-zinc-100">
          페이지를 찾을 수 없습니다
        </h1>
        <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
          요청하신 주소가 삭제되었거나 잘못 입력된 것 같아요.
        </p>
        <p className="mt-1 text-xs text-zinc-400">
          The page you&rsquo;re looking for doesn&rsquo;t exist.
        </p>

        <div className="mt-8 flex flex-wrap items-center justify-center gap-2">
          <Link
            href="/"
            className="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-emerald-700"
          >
            <HugeiconsIcon
              icon={Home01Icon}
              className="h-3.5 w-3.5"
              size="100%"
            />
            홈으로 (Home)
          </Link>
          <Link
            href="/?onboarding=1"
            className="inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-4 py-2 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon
              icon={HelpCircleIcon}
              className="h-3.5 w-3.5"
              size="100%"
            />
            시작 가이드 (Get started)
          </Link>
        </div>

        <p className="mt-8 flex items-center justify-center gap-1.5 text-[10px] text-zinc-400">
          <HugeiconsIcon
            icon={Search01Icon}
            className="h-3 w-3"
            size="100%"
          />
          <span>
            Tip: 사이드바의 검색(<kbd className="rounded border border-zinc-200 bg-white px-1 py-0.5 font-mono text-[9px] dark:border-zinc-700 dark:bg-zinc-900">⌘K</kbd>)에서 원하는 항목을 빠르게 찾을 수 있어요.
          </span>
        </p>
      </div>
    </div>
  );
}

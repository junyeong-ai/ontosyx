// Skeleton — content placeholder shown while data is fetching. Uses
// a low-contrast surface tone with a shimmer animation so the eye
// registers "loading" without the bounce-and-pop of a spinner-then-
// content swap. Variants cover the four shapes that repeat across
// the workbench: lines, cards, widget grids, and lists.
//
// Why an internal primitive (vs `react-loading-skeleton`): the
// package ships its own theme provider and CSS — both fight
// Tailwind v4's `@theme inline` token chain. A 60-line internal
// primitive matches the design system tokens with one source of
// truth and a single shimmer animation.

import { cn } from "@/lib/cn";

interface SkeletonProps {
  className?: string;
}

export function Skeleton({ className }: SkeletonProps) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "skeleton-shimmer rounded-md bg-surface-inset",
        className,
      )}
    />
  );
}

export function SkeletonText({
  lines = 3,
  className,
}: {
  lines?: number;
  className?: string;
}) {
  return (
    <div className={cn("space-y-2", className)}>
      {Array.from({ length: lines }, (_, i) => (
        <Skeleton
          key={i}
          className={cn("h-3", i === lines - 1 ? "w-2/3" : "w-full")}
        />
      ))}
    </div>
  );
}

export function SkeletonCard({ className }: SkeletonProps) {
  return (
    <div
      className={cn(
        "rounded-lg border border-divider bg-surface-base p-4",
        className,
      )}
    >
      <Skeleton className="mb-3 h-4 w-1/3" />
      <SkeletonText lines={2} />
    </div>
  );
}

export function SkeletonWidgetGrid({ count = 4 }: { count?: number }) {
  return (
    <div className="grid grid-cols-12 gap-4">
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="col-span-6">
          <div className="rounded-lg border border-divider bg-surface-base">
            <div className="border-b border-divider-soft px-3 py-2">
              <Skeleton className="h-3 w-24" />
            </div>
            <div className="p-3">
              <Skeleton className="h-[120px] w-full" />
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

export function SkeletonList({ count = 5 }: { count?: number }) {
  return (
    <div className="space-y-2">
      {Array.from({ length: count }, (_, i) => (
        <SkeletonCard key={i} />
      ))}
    </div>
  );
}

/**
 * Tabular placeholder — `rows` x `cols` evenly-spaced lines for any
 * `<table>` host while the first page is in flight.
 */
export function SkeletonTable({
  rows = 5,
  cols = 4,
}: {
  rows?: number;
  cols?: number;
}) {
  return (
    <div className="space-y-2">
      {Array.from({ length: rows }, (_, r) => (
        <div key={r} className="flex gap-3">
          {Array.from({ length: cols }, (_, c) => (
            <Skeleton key={c} className="h-4 flex-1" />
          ))}
        </div>
      ))}
    </div>
  );
}

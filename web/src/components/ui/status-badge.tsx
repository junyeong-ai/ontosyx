import { cn } from "@/lib/cn";

const DEFAULT_COLOR_MAP: Record<string, string> = {
  draft: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
  approved: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  active: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  deprecated: "bg-zinc-200 text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400",
  inactive: "bg-zinc-200 text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400",
  error: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
};

interface StatusBadgeProps {
  status: string;
  colorMap?: Record<string, string>;
  className?: string;
}

export function StatusBadge({ status, colorMap, className }: StatusBadgeProps) {
  const colors = colorMap ?? DEFAULT_COLOR_MAP;
  const colorClass =
    colors[status] ?? "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400";

  return (
    <span
      className={cn(
        "inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider",
        colorClass,
        className,
      )}
    >
      {status}
    </span>
  );
}

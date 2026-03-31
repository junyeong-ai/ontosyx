import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";

interface EmptyStateProps {
  icon?: IconSvgElement;
  title: string;
  description?: string;
  hint?: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
}

export function EmptyState({ icon, title, description, hint, action, secondaryAction }: EmptyStateProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      {icon && (
        <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
          <HugeiconsIcon icon={icon} className="h-5 w-5 text-emerald-500" size="100%" />
        </div>
      )}
      <div>
        <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300">{title}</p>
        {description && (
          <p className="mt-1 text-xs text-zinc-500">{description}</p>
        )}
      </div>
      {(action || secondaryAction) && (
        <div className="flex items-center gap-3">
          {action && (
            <button
              onClick={action.onClick}
              className="rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-2 text-xs font-medium text-emerald-700 transition-colors hover:bg-emerald-100 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-400 dark:hover:bg-emerald-950/50"
            >
              {action.label}
            </button>
          )}
          {secondaryAction && (
            <button
              onClick={secondaryAction.onClick}
              className="text-xs text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
            >
              {secondaryAction.label}
            </button>
          )}
        </div>
      )}
      {hint && (
        <p className="text-[11px] text-zinc-400">{hint}</p>
      )}
    </div>
  );
}

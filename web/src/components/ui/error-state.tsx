// ErrorState — communicates a *failure* state (network down, auth
// missing, server crash) and offers a retry. Distinct from EmptyState
// which communicates a *no-data* state for an otherwise-healthy
// fetch. The two carry different colour tone, copy register, and
// recovery affordance — keeping them in separate components prevents
// callers from accidentally signalling "broken" when they meant
// "empty list", which erodes user trust.

import { HugeiconsIcon } from "@hugeicons/react";
import { AlertCircleIcon } from "@hugeicons/core-free-icons";

interface ErrorStateProps {
  title: string;
  description?: string;
  onRetry?: () => void;
  retryLabel?: string;
}

export function ErrorState({
  title,
  description,
  onRetry,
  retryLabel,
}: ErrorStateProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-full bg-danger-surface">
        <HugeiconsIcon
          icon={AlertCircleIcon}
          className="h-5 w-5 text-danger-foreground"
          size="100%"
        />
      </div>
      <div>
        <p className="text-sm font-medium text-foreground">{title}</p>
        {description && (
          <p className="mt-1 text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      {onRetry && retryLabel && (
        <button
          onClick={onRetry}
          className="rounded-lg border border-divider bg-surface-base px-4 py-2 text-xs font-medium text-foreground transition-colors hover:bg-surface-inset"
        >
          {retryLabel}
        </button>
      )}
    </div>
  );
}

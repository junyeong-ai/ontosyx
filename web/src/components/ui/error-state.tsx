// ErrorState — communicates a *failure* state (network down, auth
// missing, server crash) and offers a retry. Distinct from EmptyState
// which communicates a *no-data* state for an otherwise-healthy
// fetch. The two carry different colour tone, copy register, and
// recovery affordance — keeping them in separate components prevents
// callers from accidentally signalling "broken" when they meant
// "empty list", which erodes user trust.

import { AlertCircle } from "lucide-react";
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
    <div className="flex h-full items-center justify-center p-8">
      <div className="flex max-w-md flex-col items-center gap-4 rounded-2xl border border-danger-border/40 bg-danger-surface/40 px-8 py-7 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-danger-surface ring-4 ring-danger-surface/30">
          <AlertCircle className="h-8 w-8 text-danger-foreground" />
        </div>
        <div>
          <p className="text-sm font-semibold text-foreground-strong">{title}</p>
          {description && (
            <p className="mt-1 text-xs text-foreground-muted">{description}</p>
          )}
        </div>
        {onRetry && retryLabel && (
          <button type="button"
            onClick={onRetry}
            className="rounded-lg border border-divider bg-surface-base px-4 py-2 text-xs font-medium text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset"
          >
            {retryLabel}
          </button>
        )}
      </div>
    </div>
  );
}

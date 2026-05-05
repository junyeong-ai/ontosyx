"use client";

// FacetShell — uniform loading / error / empty / ready chrome for
// inspector facets.
//
// Every facet that pulls async data (samples, lineage, change-log,
// quality streams) reaches the same five intermediate states before
// it can render: loading, error, empty, ready, and locked-by-other.
// Without a primitive, each facet rolls its own skeleton + retry
// button + empty CTA, and the visual register drifts between panes.
// `FacetShell` collapses all of that into one discriminated union so
// the *shape* of inspector loading state is owned by the design
// system, not the facet author.
//
// Facets that don't need it can still render their content directly;
// the shell is opt-in. Facets that DO use it MUST exhaustively
// describe every state they support — TypeScript checks the kind
// switch is complete.

import type { ReactNode } from "react";
import type { LucideIcon as IconSvgElement } from "lucide-react";
import { ErrorState } from "@/components/ui/error-state";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import { SkeletonText } from "@/components/ui/skeleton";

export type FacetState =
  | { kind: "loading" }
  | {
      kind: "error";
      title: string;
      description?: string;
      onRetry?: () => void;
      retryLabel?: string;
    }
  | {
      kind: "empty";
      icon?: IconSvgElement;
      title: string;
      description?: string;
    }
  | { kind: "ready"; children: ReactNode };

export function FacetShell({ state }: { state: FacetState }) {
  switch (state.kind) {
    case "loading":
      return (
        <div className="space-y-2 p-3">
          <SkeletonText lines={4} />
        </div>
      );
    case "error":
      return (
        <ErrorState
          title={state.title}
          description={state.description}
          onRetry={state.onRetry}
          retryLabel={state.retryLabel}
        />
      );
    case "empty":
      return (
        <div className="flex flex-col items-center gap-2 px-3 py-8 text-center">
          {state.icon && (
            <div className="flex h-9 w-9 items-center justify-center rounded-full bg-surface-inset">
              <DynamicIcon as={state.icon} className="h-4 w-4 text-foreground-muted" />
            </div>
          )}
          <p className="text-xs font-medium text-foreground">{state.title}</p>
          {state.description && (
            <p className="max-w-xs text-2xs text-foreground-muted">
              {state.description}
            </p>
          )}
        </div>
      );
    case "ready":
      return <>{state.children}</>;
  }
}

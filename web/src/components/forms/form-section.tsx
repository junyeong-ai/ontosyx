"use client";

// FormSection — lightweight, web-standard fieldset+legend group used
// by every structured form. Industry pattern (Linear / Notion /
// Sanity / Stripe Dashboard): dense forms group fields with a
// subtle border and a small uppercase legend cutting through it,
// not a heavy card with chevrons. The card pattern is reserved for
// page-level tiles where the chrome itself is meaningful (settings
// dashboard panels, metric tiles).
//
// `collapsible` opts into a `<details>`-style toggle for optional
// or rarely-edited groups (e.g. governance / origin metadata) — the
// lightweight equivalent of CollapsibleSection without the card
// chrome.

import { type ReactNode, useId } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";

import { cn } from "@/lib/cn";

interface FormSectionProps {
  title: string;
  description?: string;
  children: ReactNode;
  /** When true, the legend is wrapped in a button toggle. */
  collapsible?: boolean;
  /** Default open state for collapsible sections. */
  defaultOpen?: boolean;
  /** Tighten the inner gap when the section holds a single grid. */
  className?: string;
}

export function FormSection({
  title,
  description,
  children,
  collapsible = false,
  defaultOpen = true,
  className,
}: FormSectionProps) {
  const summaryId = useId();
  if (collapsible) {
    return (
      <details
        open={defaultOpen}
        className={cn(
          "group rounded-md border border-divider bg-surface-base/40",
          className,
        )}
      >
        <summary
          id={summaryId}
          className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 text-2xs font-medium text-foreground [&::-webkit-details-marker]:hidden"
        >
          <HugeiconsIcon
            icon={ArrowRight01Icon}
            className="h-3 w-3 group-open:hidden"
            size="100%"
          />
          <HugeiconsIcon
            icon={ArrowDown01Icon}
            className="hidden h-3 w-3 group-open:inline"
            size="100%"
          />
          <span>{title}</span>
          {description && (
            <span className="ms-2 text-2xs text-foreground-subtle">
              {description}
            </span>
          )}
        </summary>
        <div className="flex flex-col gap-2 px-3 pb-3 pt-1">{children}</div>
      </details>
    );
  }
  return (
    <fieldset
      className={cn(
        "flex flex-col gap-2 rounded border border-divider p-3",
        className,
      )}
    >
      <legend className="px-1 text-2xs font-medium text-foreground">
        {title}
      </legend>
      {description && (
        <p className="-mt-1 text-2xs text-foreground-subtle">{description}</p>
      )}
      {children}
    </fieldset>
  );
}

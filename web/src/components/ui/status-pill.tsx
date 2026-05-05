"use client";

// StatusPill — clickable status indicator with dropdown picker.
// Replaces the radio-stack pattern (3 vertical buttons) for any
// "lifecycle" / "state" / "severity" field on detail editors.
// Industry pattern: Linear issue status, GitHub PR status, Stripe
// subscription state — single pill that expands on click into a
// short list of options. One click to change, takes 1 row instead
// of 3.
//
// The component is type-generic over the option key — pass the
// schema enum directly. `tone` per option drives the colour; if
// omitted, falls back to "neutral".

import { type ReactNode, useState } from "react";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { cn } from "@/lib/cn";

export interface StatusPillOption<K extends string> {
  key: K;
  label: string;
  tone?: StatusTone;
  description?: string;
}

interface StatusPillProps<K extends string> {
  value: K;
  options: readonly StatusPillOption<K>[];
  onChange: (key: K) => void;
  disabled?: boolean;
  /** ARIA label for the trigger when the rendered label is generic. */
  ariaLabel?: string;
}

export function StatusPill<K extends string>({
  value,
  options,
  onChange,
  disabled,
  ariaLabel,
}: StatusPillProps<K>) {
  const [open, setOpen] = useState(false);
  const active = options.find((o) => o.key === value) ?? options[0];

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        className={cn(
          "inline-flex items-center gap-1.5 rounded-full text-2xs font-medium transition-colors duration-[var(--duration-quick)]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring-default",
          disabled && "pointer-events-none opacity-60",
        )}
        aria-label={ariaLabel}
      >
        <StatusBadge tone={active?.tone ?? "neutral"} variant="soft" size="sm">
          <span className="flex items-center gap-1">
            <Dot tone={active?.tone ?? "neutral"} />
            {active?.label}
            <span aria-hidden className="text-foreground-muted">
              ▾
            </span>
          </span>
        </StatusBadge>
      </PopoverTrigger>
      <PopoverContent className="min-w-[14rem] p-1">
        <ul role="listbox" className="flex flex-col gap-0.5">
          {options.map((option) => (
            <li key={option.key}>
              <button
                type="button"
                role="option"
                aria-selected={option.key === value}
                onClick={() => {
                  onChange(option.key);
                  setOpen(false);
                }}
                className={cn(
                  "flex w-full items-start gap-2 rounded px-2 py-1.5 text-left text-xs transition-colors duration-[var(--duration-quick)]",
                  option.key === value
                    ? "bg-brand-surface"
                    : "hover:bg-surface-hover",
                )}
              >
                <Dot tone={option.tone ?? "neutral"} className="mt-1 shrink-0" />
                <div className="min-w-0 flex-1">
                  <p
                    className={cn(
                      "font-medium",
                      option.key === value
                        ? "text-brand-foreground"
                        : "text-foreground-strong",
                    )}
                  >
                    {option.label}
                  </p>
                  {option.description && (
                    <p className="mt-0.5 text-2xs text-foreground-muted">
                      {option.description}
                    </p>
                  )}
                </div>
              </button>
            </li>
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  );
}

function Dot({ tone, className }: { tone: StatusTone; className?: string }): ReactNode {
  const toneClass: Record<StatusTone, string> = {
    neutral: "bg-foreground-muted",
    brand: "bg-brand-solid",
    success: "bg-brand-solid",
    warning: "bg-warning-foreground",
    danger: "bg-danger-solid",
    info: "bg-info-foreground",
    concept: "bg-concept-foreground",
  };
  return (
    <span
      aria-hidden
      className={cn("h-1.5 w-1.5 shrink-0 rounded-full", toneClass[tone], className)}
    />
  );
}

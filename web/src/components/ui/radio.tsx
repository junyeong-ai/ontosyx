"use client";

import { forwardRef, type InputHTMLAttributes, type ReactNode } from "react";
import { cn } from "@/lib/cn";

interface RadioProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "children"> {
  /** Visible label. Wraps the input in a `<label>` for implicit association. */
  label: ReactNode;
  /** Stacks the input + label horizontally (default) or vertically. */
  layout?: "horizontal" | "vertical";
}

/**
 * Single radio button paired with its label. Pass the same `name` to
 * a group of `<Radio>` instances to wire mutual exclusion.
 */
export const Radio = forwardRef<HTMLInputElement, RadioProps>(
  ({ className, label, layout = "horizontal", ...props }, ref) => (
    <label
      className={cn(
        "inline-flex cursor-pointer items-center gap-1.5 text-foreground",
        layout === "vertical" && "flex-col items-start gap-0.5",
      )}
    >
      <input
        ref={ref}
        type="radio"
        className={cn(
          "h-3.5 w-3.5 cursor-pointer accent-brand-foreground",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
          "disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        {...props}
      />
      <span className="text-xs">{label}</span>
    </label>
  ),
);

Radio.displayName = "Radio";

interface RadioGroupProps {
  /** ARIA-visible name shared by every option. */
  ariaLabel: string;
  className?: string;
  children: ReactNode;
}

/**
 * Wraps a set of `<Radio>` siblings with the right ARIA semantics.
 * Each child must carry its own `name`, `value`, and `checked` props.
 */
export function RadioGroup({ ariaLabel, className, children }: RadioGroupProps) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn("flex flex-wrap items-center gap-3", className)}
    >
      {children}
    </div>
  );
}

interface RadioCardProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "children" | "title"> {
  /** Card title — visually weighted as the primary affordance. */
  title: ReactNode;
  /** Optional supporting copy below the title. */
  hint?: ReactNode;
}

/**
 * Card-styled radio option — the input itself is `sr-only`, the
 * surrounding `<label>` is the visual target. Onboarding wizards,
 * source-kind pickers, and tier selectors use this idiom across
 * the workbench. Selection state lifts the card to brand chrome
 * the way every modern picker (Linear, Notion, Vercel) does.
 *
 * Pass the same `name` to every option in a group, plus per-option
 * `value` / `checked` / `onChange` — the surrounding `<RadioGroup>`
 * provides the ARIA scaffolding.
 */
export const RadioCard = forwardRef<HTMLInputElement, RadioCardProps>(
  ({ className, title, hint, checked, ...props }, ref) => (
    <label
      className={cn(
        "block cursor-pointer rounded border px-3 py-3 text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] focus-within:ring-2 focus-within:ring-brand-foreground/40",
        checked
          ? "border-brand-foreground bg-brand-surface text-brand-foreground"
          : "border-divider bg-surface-base text-foreground-muted hover:bg-surface-raised",
        className,
      )}
    >
      <input
        ref={ref}
        type="radio"
        checked={checked}
        className="sr-only"
        {...props}
      />
      <p className="font-medium">{title}</p>
      {hint && <p className="mt-0.5 text-2xs text-foreground-muted">{hint}</p>}
    </label>
  ),
);

RadioCard.displayName = "RadioCard";

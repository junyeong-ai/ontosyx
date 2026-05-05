"use client";

import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { cn } from "@/lib/cn";
import { Spinner } from "./spinner";
import { Tooltip } from "./tooltip";

type ButtonVariant = "default" | "primary" | "ghost" | "outline" | "danger";
type ButtonSize = "xs" | "sm" | "md" | "lg" | "icon" | "icon-sm";

const variantClass: Record<ButtonVariant, string> = {
  default:
    "bg-foreground text-foreground-onbrand hover:opacity-90",
  primary:
    "bg-brand-solid text-foreground-onbrand hover:bg-brand-solid-hover",
  ghost:
    "text-foreground-muted hover:bg-surface-inset hover:text-foreground-strong",
  outline:
    "border border-divider text-foreground hover:bg-surface-inset",
  danger:
    "bg-danger-solid text-foreground-on-accent hover:bg-danger-solid-hover",
};

const sizeClass: Record<ButtonSize, string> = {
  xs: "h-7 px-2 text-xs",
  sm: "h-8 px-3 text-xs",
  md: "h-9 px-4 text-sm",
  lg: "h-10 px-6 text-sm",
  icon: "h-9 w-9",
  "icon-sm": "h-7 w-7",
};

const spinnerSizeFor: Record<ButtonSize, "xs" | "sm" | "md"> = {
  xs: "xs",
  sm: "xs",
  md: "sm",
  lg: "sm",
  icon: "sm",
  "icon-sm": "xs",
};

const baseClass =
  "inline-flex items-center justify-center gap-1.5 rounded-md font-medium select-none " +
  "transition-[colors,transform] duration-[var(--duration-quick)] ease-[var(--ease-out)] " +
  "active:scale-[0.97] " +
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-1 " +
  "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 disabled:active:scale-100";

/**
 * Compose the canonical button class string for an arbitrary element.
 *
 * Use this on `<a>` / `<Link>` / other non-`<button type="button">` elements that
 * should *visually* read as a button. Anchor-as-button avoids the
 * `<a><button type="button"/></a>` anti-pattern (which both Vercel and Linear
 * disallow — invalid HTML, broken keyboard semantics, double focus).
 *
 * **Trade-off:** elements styled with `buttonStyles` do NOT get
 * `loading`, `tooltip`, `leadingIcon`, or `trailingIcon` props — those
 * are `<Button>`-only since they require React state / wrapping. If
 * the call-site needs any of those, stay with `<Button>` and route
 * navigation imperatively (`router.push`) on click.
 *
 * @example
 *   <Link href="/" className={buttonStyles({ variant: "primary" })}>
 *     Home
 *   </Link>
 */
export function buttonStyles({
  variant = "default",
  size = "md",
  className,
}: {
  variant?: ButtonVariant;
  size?: ButtonSize;
  className?: string;
} = {}): string {
  return cn(baseClass, variantClass[variant], sizeClass[size], className);
}

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /**
   * When `true`, replaces the leading content with a spinner, sets
   * `aria-busy`, and blocks click. Mutation buttons should drive
   * this from their `useMutation().isPending` flag.
   */
  loading?: boolean;
  /**
   * Hover/focus tooltip. Wraps the rendered button in a `Tooltip`
   * primitive so disabled buttons stay self-documenting (the
   * standard explanation for *why* the action is unavailable).
   */
  tooltip?: ReactNode;
  /** Inline icon shown before the label. Hidden during `loading`. */
  leadingIcon?: ReactNode;
  /** Inline icon shown after the label. */
  trailingIcon?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant = "default",
      size = "md",
      loading = false,
      tooltip,
      leadingIcon,
      trailingIcon,
      disabled,
      children,
      ...props
    },
    ref,
  ) => {
    const button = (
      <button type="button"
        ref={ref}
        disabled={disabled || loading}
        aria-busy={loading || undefined}
        className={buttonStyles({ variant, size, className })}
        {...props}
      >
        {loading ? (
          <Spinner size={spinnerSizeFor[size]} className="shrink-0" />
        ) : (
          leadingIcon && <span className="shrink-0">{leadingIcon}</span>
        )}
        {children}
        {!loading && trailingIcon && (
          <span className="shrink-0">{trailingIcon}</span>
        )}
      </button>
    );

    if (tooltip) {
      return <Tooltip content={tooltip}>{button}</Tooltip>;
    }
    return button;
  },
);

Button.displayName = "Button";

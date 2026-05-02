"use client";

import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/cn";

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
    "bg-danger-solid text-foreground-onbrand hover:bg-danger-solid-hover",
};

const sizeClass: Record<ButtonSize, string> = {
  xs: "h-7 px-2 text-xs",
  sm: "h-8 px-3 text-xs",
  md: "h-9 px-4 text-sm",
  lg: "h-10 px-6 text-sm",
  icon: "h-9 w-9",
  "icon-sm": "h-7 w-7",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "default", size = "md", ...props }, ref) => (
    <button
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium",
        "transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-1",
        "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50",
        variantClass[variant],
        sizeClass[size],
        className,
      )}
      {...props}
    />
  ),
);

Button.displayName = "Button";

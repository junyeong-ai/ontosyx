import { type HTMLAttributes, forwardRef } from "react";
import { cn } from "@/lib/cn";

type CardVariant = "surface" | "raised" | "inset";
type CardPadding = "none" | "sm" | "md" | "lg";

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  variant?: CardVariant;
  padding?: CardPadding;
  /**
   * Adds hover/focus affordances for clickable cards (project tile,
   * recipe card). Pair with `tabIndex={0}` + `role="button"` on the
   * element when the card itself is the click target.
   */
  interactive?: boolean;
}

const variantClass: Record<CardVariant, string> = {
  surface: "bg-surface-base border border-divider",
  raised:  "bg-surface-base border border-divider shadow-1",
  inset:   "bg-surface-raised border border-divider",
};

const paddingClass: Record<CardPadding, string> = {
  none: "",
  sm:   "p-3",
  md:   "p-4",
  lg:   "p-6",
};

const interactiveClass = cn(
  "cursor-pointer transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
  "hover:bg-surface-inset",
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-1",
);

const CardRoot = forwardRef<HTMLDivElement, CardProps>(
  ({ variant = "surface", padding = "md", interactive, className, ...rest }, ref) => (
    <div
      ref={ref}
      className={cn(
        "rounded-lg",
        variantClass[variant],
        paddingClass[padding],
        interactive && interactiveClass,
        className,
      )}
      {...rest}
    />
  ),
);
CardRoot.displayName = "Card";

const CardHeader = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...rest }, ref) => (
    <div
      ref={ref}
      className={cn(
        "flex items-center justify-between gap-4 border-b border-divider px-4 py-3",
        className,
      )}
      {...rest}
    />
  ),
);
CardHeader.displayName = "Card.Header";

const CardBody = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...rest }, ref) => (
    <div ref={ref} className={cn("px-4 py-4", className)} {...rest} />
  ),
);
CardBody.displayName = "Card.Body";

const CardFooter = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...rest }, ref) => (
    <div
      ref={ref}
      className={cn(
        "flex items-center justify-end gap-2 border-t border-divider px-4 py-3",
        className,
      )}
      {...rest}
    />
  ),
);
CardFooter.displayName = "Card.Footer";

const CardTitle = forwardRef<HTMLHeadingElement, HTMLAttributes<HTMLHeadingElement>>(
  ({ className, ...rest }, ref) => (
    <h3
      ref={ref}
      className={cn("text-sm font-semibold text-foreground-strong", className)}
      {...rest}
    />
  ),
);
CardTitle.displayName = "Card.Title";

const CardDescription = forwardRef<HTMLParagraphElement, HTMLAttributes<HTMLParagraphElement>>(
  ({ className, ...rest }, ref) => (
    <p
      ref={ref}
      className={cn("text-xs text-foreground-muted", className)}
      {...rest}
    />
  ),
);
CardDescription.displayName = "Card.Description";

type CardCompound = typeof CardRoot & {
  Header: typeof CardHeader;
  Body: typeof CardBody;
  Footer: typeof CardFooter;
  Title: typeof CardTitle;
  Description: typeof CardDescription;
};

const Card = CardRoot as CardCompound;
Card.Header = CardHeader;
Card.Body = CardBody;
Card.Footer = CardFooter;
Card.Title = CardTitle;
Card.Description = CardDescription;

export { Card };

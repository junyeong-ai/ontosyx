import {
  type HTMLAttributes,
  type KeyboardEvent,
  forwardRef,
} from "react";
import { cn } from "@/lib/cn";

type CardVariant = "surface" | "raised" | "inset";
type CardPadding = "none" | "sm" | "md" | "lg";

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  variant?: CardVariant;
  padding?: CardPadding;
  /**
   * Marks the card as a clickable target. Combined with `onClick`,
   * the primitive injects `role="button"`, `tabIndex={0}`, and
   * Enter/Space keyboard activation — consumers do not reimplement
   * the a11y boilerplate. `cursor-pointer` + a hover/focus-visible
   * ring drop in for free.
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
  (
    {
      variant = "surface",
      padding = "md",
      interactive,
      className,
      onClick,
      onKeyDown,
      role,
      tabIndex,
      ...rest
    },
    ref,
  ) => {
    const clickable = Boolean(interactive && onClick);
    const handleKeyDown = clickable
      ? (event: KeyboardEvent<HTMLDivElement>) => {
          // Only activate when the keystroke originated on the Card
          // root, not on a focused descendant — pressing Space inside
          // a child input would otherwise re-trigger the card click.
          if (
            (event.key === "Enter" || event.key === " ") &&
            event.target === event.currentTarget
          ) {
            event.preventDefault();
            onClick?.(
              event as unknown as React.MouseEvent<HTMLDivElement>,
            );
          }
          onKeyDown?.(event);
        }
      : onKeyDown;
    return (
      <div
        ref={ref}
        className={cn(
          "rounded-lg",
          variantClass[variant],
          paddingClass[padding],
          interactive && interactiveClass,
          className,
        )}
        onClick={onClick}
        onKeyDown={handleKeyDown}
        role={role ?? (clickable ? "button" : undefined)}
        tabIndex={tabIndex ?? (clickable ? 0 : undefined)}
        {...rest}
      />
    );
  },
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

type HeadingTag = "h1" | "h2" | "h3" | "h4" | "h5" | "h6";

interface CardTitleProps extends HTMLAttributes<HTMLHeadingElement> {
  /**
   * Heading level. Defaults to `h3` because cards typically nest
   * under a page-level `h1` and a section `h2`. Override when the
   * card is itself a top-level region or a deeper sub-section so
   * the document hierarchy doesn't skip levels (WCAG 2.4.6).
   */
  as?: HeadingTag;
}

const CardTitle = forwardRef<HTMLHeadingElement, CardTitleProps>(
  ({ as: Tag = "h3", className, ...rest }, ref) => (
    <Tag
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

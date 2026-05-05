"use client";

import {
  forwardRef,
  useCallback,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import { cn } from "@/lib/cn";

interface CheckboxProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "children"> {
  /** Visible label. Wraps the input in a `<label>` for implicit association. */
  label?: ReactNode;
  /** Optional helper text rendered below the label. */
  hint?: ReactNode;
  /** Aligns the input with the first line when label wraps to multiple rows. */
  align?: "center" | "start";
  /**
   * Tri-state visual: true overlays the indeterminate dash on the
   * input. The DOM property is imperative-only on `HTMLInputElement`,
   * so the primitive owns the ref-callback bridge — call sites just
   * pass the boolean.
   */
  indeterminate?: boolean;
}

/**
 * Standalone checkbox paired with its label. Pass `label` to render the
 * `<input>` inside a `<label>` element for implicit association; omit it
 * when wrapping a control inside an existing label/fieldset.
 */
export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(
  (
    { className, label, hint, align = "center", indeterminate, ...props },
    forwardedRef,
  ) => {
    const setRefs = useCallback(
      (el: HTMLInputElement | null) => {
        if (el) {
          el.indeterminate = !!indeterminate;
        }
        if (typeof forwardedRef === "function") {
          forwardedRef(el);
        } else if (forwardedRef) {
          forwardedRef.current = el;
        }
      },
      [forwardedRef, indeterminate],
    );

    const input = (
      <input
        ref={setRefs}
        type="checkbox"
        className={cn(
          "h-3.5 w-3.5 shrink-0 cursor-pointer rounded accent-brand-foreground",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
          "disabled:cursor-not-allowed disabled:opacity-50",
          align === "start" && "mt-0.5",
          className,
        )}
        {...props}
      />
    );

    if (!label) return input;

    return (
      <label
        className={cn(
          "inline-flex cursor-pointer gap-1.5 text-foreground",
          align === "center" ? "items-center" : "items-start",
        )}
      >
        {input}
        <span className="flex flex-col gap-0.5">
          <span className="text-xs">{label}</span>
          {hint && (
            <span className="text-2xs text-foreground-muted">{hint}</span>
          )}
        </span>
      </label>
    );
  },
);

Checkbox.displayName = "Checkbox";

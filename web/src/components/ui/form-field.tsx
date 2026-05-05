"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";

interface FormFieldProps {
  label: string;
  required?: boolean;
  error?: string;
  hint?: string;
  /** Visually hide the label without dropping the screen-reader
   *  announcement. */
  hideLabel?: boolean;
  children: React.ReactNode;
  className?: string;
}

/**
 * Labelled form field container.
 *
 * A single `<label>` wraps the children, which creates an **implicit
 * label association** with the first form control inside. Raw
 * `<input>`, `<select>`, or `<textarea>` children become accessible to
 * screen readers automatically — no `id`/`htmlFor` wiring required.
 *
 * Validation: when `error` is set, the wrapped control inherits the
 * danger tone (via `[aria-invalid]` selectors on the input primitives)
 * and the message is announced through `role="alert"`. Hint text is
 * suppressed while an error is active so the reader hears the
 * actionable message first.
 *
 * Required marker: the visual `*` is `aria-hidden`; an `sr-only`
 * "required" string is appended so screen readers announce "{label},
 * required" without the asterisk being read out as punctuation.
 */
export function FormField({
  label,
  required,
  error,
  hint,
  hideLabel,
  children,
  className,
}: FormFieldProps) {
  const tCommon = useTranslations("common.formField");
  return (
    <label className={cn("block space-y-1.5", className)}>
      <span
        className={cn(
          "block text-xs font-medium text-foreground-muted",
          hideLabel && "sr-only",
        )}
      >
        {label}
        {required && (
          <>
            <span className="ms-0.5 text-danger-foreground" aria-hidden="true">
              *
            </span>
            <span className="sr-only"> {tCommon("required")}</span>
          </>
        )}
      </span>
      {children}
      {error && (
        <span
          className="block text-2xs font-medium text-danger-foreground"
          role="alert"
        >
          {error}
        </span>
      )}
      {hint && !error && (
        <span className="block text-2xs text-foreground-muted">{hint}</span>
      )}
    </label>
  );
}

"use client";

import { cn } from "@/lib/cn";

interface FormFieldProps {
  label: string;
  required?: boolean;
  error?: string;
  hint?: string;
  children: React.ReactNode;
  className?: string;
}

/**
 * Labelled form field container.
 *
 * A single `<label>` wraps the children, which creates an **implicit
 * label association** with the first form control inside. This means
 * raw `<input>`, `<select>`, or `<textarea>` children become accessible
 * to screen readers automatically — no `id`/`htmlFor` wiring required.
 * Non-form-control children (decorative `<div>`s, icons) are ignored by
 * the association.
 *
 * Why implicit over `htmlFor`: the prior API forced every caller to
 * generate and pass a stable id. In practice nearly nobody did, so
 * 100% of `<FormField>` consumers rendered visually labelled inputs
 * that were **unlabelled for assistive tech**. Axe flagged these as a
 * critical `select-name` violation on the design page. Implicit
 * association closes the gap at the container level.
 */
export function FormField({
  label,
  required,
  error,
  hint,
  children,
  className,
}: FormFieldProps) {
  return (
    <label className={cn("block space-y-1", className)}>
      <span className="block text-xs font-medium text-zinc-600 dark:text-zinc-400">
        {label}
        {required && (
          <span className="ml-0.5 text-red-500" aria-label="required">*</span>
        )}
      </span>
      {children}
      {error && (
        <span
          className="block text-[11px] text-red-500 dark:text-red-400"
          role="alert"
        >
          {error}
        </span>
      )}
      {hint && !error && (
        <span className="block text-[11px] text-zinc-400">{hint}</span>
      )}
    </label>
  );
}

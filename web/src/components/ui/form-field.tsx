"use client";

import { cn } from "@/lib/cn";

interface FormFieldProps {
  label: string;
  required?: boolean;
  error?: string;
  hint?: string;
  htmlFor?: string;
  children: React.ReactNode;
  className?: string;
}

export function FormField({
  label,
  required,
  error,
  hint,
  htmlFor,
  children,
  className,
}: FormFieldProps) {
  return (
    <div className={cn("space-y-1", className)}>
      <label
        htmlFor={htmlFor}
        className="block text-xs font-medium text-zinc-600 dark:text-zinc-400"
      >
        {label}
        {required && (
          <span className="ml-0.5 text-red-500" aria-label="required">*</span>
        )}
      </label>
      {children}
      {error && (
        <p className="text-[11px] text-red-500 dark:text-red-400" role="alert">
          {error}
        </p>
      )}
      {hint && !error && (
        <p className="text-[11px] text-zinc-400">{hint}</p>
      )}
    </div>
  );
}

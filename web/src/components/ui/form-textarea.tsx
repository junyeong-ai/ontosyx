"use client";

import { forwardRef, type TextareaHTMLAttributes } from "react";
import { cn } from "@/lib/cn";

interface FormTextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  error?: boolean;
}

export const FormTextarea = forwardRef<HTMLTextAreaElement, FormTextareaProps>(
  ({ className, error, ...props }, ref) => (
    <textarea
      ref={ref}
      aria-invalid={error || undefined}
      className={cn(
        "w-full rounded-md border bg-transparent px-3 py-2 text-sm",
        "outline-none transition-colors",
        error
          ? "border-red-400 focus:border-red-500 focus:ring-1 focus:ring-red-500/50 dark:border-red-500 dark:focus:border-red-400 dark:focus:ring-red-400/50"
          : "border-zinc-300 focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500/50 dark:border-zinc-600 dark:focus:border-emerald-400 dark:focus:ring-emerald-400/50",
        className,
      )}
      {...props}
    />
  ),
);

FormTextarea.displayName = "FormTextarea";

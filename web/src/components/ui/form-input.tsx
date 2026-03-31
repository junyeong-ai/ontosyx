"use client";

import {
  forwardRef,
  type InputHTMLAttributes,
  type TextareaHTMLAttributes,
  type SelectHTMLAttributes,
} from "react";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Base FormInput (existing)
// ---------------------------------------------------------------------------

interface FormInputProps extends InputHTMLAttributes<HTMLInputElement> {
  error?: boolean;
}

export const FormInput = forwardRef<HTMLInputElement, FormInputProps>(
  ({ className, error, ...props }, ref) => (
    <input
      ref={ref}
      aria-invalid={error || undefined}
      className={cn(
        "w-full rounded-md border bg-transparent px-3 py-1.5 text-sm",
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

FormInput.displayName = "FormInput";

// ---------------------------------------------------------------------------
// Settings-style labeled form controls
// Uses the compact text-xs + border-zinc-200 pattern from settings pages
// ---------------------------------------------------------------------------

const inputBase =
  "mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900";

const labelBase =
  "text-[10px] font-semibold uppercase tracking-wider text-zinc-500";

interface SettingsInputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
}

export const SettingsInput = forwardRef<HTMLInputElement, SettingsInputProps>(
  ({ label, className, ...props }, ref) => {
    const input = (
      <input ref={ref} className={cn(inputBase, className)} {...props} />
    );
    if (!label) return input;
    return (
      <div>
        <label className={labelBase}>{label}</label>
        {input}
      </div>
    );
  },
);

SettingsInput.displayName = "SettingsInput";

interface SettingsTextareaProps
  extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
}

export const SettingsTextarea = forwardRef<
  HTMLTextAreaElement,
  SettingsTextareaProps
>(({ label, className, ...props }, ref) => {
  const textarea = (
    <textarea ref={ref} className={cn(inputBase, className)} {...props} />
  );
  if (!label) return textarea;
  return (
    <div>
      <label className={labelBase}>{label}</label>
      {textarea}
    </div>
  );
});

SettingsTextarea.displayName = "SettingsTextarea";

interface SettingsSelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  children: React.ReactNode;
}

export const SettingsSelect = forwardRef<
  HTMLSelectElement,
  SettingsSelectProps
>(({ label, className, children, ...props }, ref) => {
  const select = (
    <select ref={ref} className={cn(inputBase, className)} {...props}>
      {children}
    </select>
  );
  if (!label) return select;
  return (
    <div>
      <label className={labelBase}>{label}</label>
      {select}
    </div>
  );
});

SettingsSelect.displayName = "SettingsSelect";

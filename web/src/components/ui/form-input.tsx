"use client";

import {
  forwardRef,
  type InputHTMLAttributes,
  type TextareaHTMLAttributes,
  type SelectHTMLAttributes,
} from "react";
import { Switch as BaseSwitch } from "@base-ui/react/switch";
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
        "w-full rounded-md border bg-surface-base px-3 py-1.5 text-sm text-foreground-strong",
        "outline-none transition-colors duration-[var(--duration-quick)]",
        "placeholder:text-foreground-subtle",
        error
          ? "border-danger-border focus:border-danger-foreground focus:ring-1 focus:ring-danger-foreground/40"
          : "border-divider focus:border-brand-foreground focus:ring-1 focus:ring-brand-foreground/40",
        className,
      )}
      {...props}
    />
  ),
);

FormInput.displayName = "FormInput";

const inputBase =
  "w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs text-foreground-strong placeholder:text-foreground-subtle transition-colors duration-[var(--duration-quick)] focus:border-brand-foreground focus:outline-none focus:ring-1 focus:ring-brand-foreground/40";

const labelBase =
  "text-2xs font-semibold uppercase tracking-wider text-foreground-muted";

/**
 * Shared shape for the labelled settings controls. `label` is REQUIRED
 * so every control has an accessible name. Pass `hideLabel` for designs
 * that prefer no visible caption (e.g. toolbar filters); the label is
 * then visually hidden via `sr-only` but still announced by screen
 * readers. The components use **implicit label association** — the
 * outer `<label>` wraps the control — so we do NOT need to thread an
 * `id`/`htmlFor` pair through every caller. Same pattern as `FormField`.
 */
interface LabelledFieldProps {
  label: string;
  hideLabel?: boolean;
}

function FieldLabelText({
  label,
  hideLabel,
}: LabelledFieldProps) {
  return (
    <span className={cn("block", labelBase, hideLabel && "sr-only")}>
      {label}
    </span>
  );
}

type SettingsInputProps = InputHTMLAttributes<HTMLInputElement> & LabelledFieldProps;

export const SettingsInput = forwardRef<HTMLInputElement, SettingsInputProps>(
  ({ label, hideLabel, className, ...props }, ref) => (
    <label className="block">
      <FieldLabelText label={label} hideLabel={hideLabel} />
      <input
        ref={ref}
        className={cn(inputBase, className)}
        {...props}
      />
    </label>
  ),
);

SettingsInput.displayName = "SettingsInput";

type SettingsTextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> &
  LabelledFieldProps;

export const SettingsTextarea = forwardRef<
  HTMLTextAreaElement,
  SettingsTextareaProps
>(({ label, hideLabel, className, ...props }, ref) => (
  <label className="block">
    <FieldLabelText label={label} hideLabel={hideLabel} />
    <textarea
      ref={ref}
      className={cn(inputBase, className)}
      {...props}
    />
  </label>
));

SettingsTextarea.displayName = "SettingsTextarea";

type SettingsSelectProps = SelectHTMLAttributes<HTMLSelectElement> &
  LabelledFieldProps & { children: React.ReactNode };

export const SettingsSelect = forwardRef<
  HTMLSelectElement,
  SettingsSelectProps
>(({ label, hideLabel, className, children, ...props }, ref) => (
  <label className="block">
    <FieldLabelText label={label} hideLabel={hideLabel} />
    <div className="relative">
      <select
        ref={ref}
        className={cn(inputBase, "appearance-none pr-8", className)}
        {...props}
      >
        {children}
      </select>
      <span className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center">
        <svg
          width="10"
          height="10"
          viewBox="0 0 10 10"
          fill="none"
          className="text-muted-foreground"
        >
          <path
            d="M2.5 3.75L5 6.25L7.5 3.75"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
    </div>
  </label>
));

SettingsSelect.displayName = "SettingsSelect";

// ---------------------------------------------------------------------------
// Settings-style toggle switch (Base UI)
// ---------------------------------------------------------------------------

interface SettingsSwitchProps {
  label?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}

export function SettingsSwitch({
  label,
  checked,
  onChange,
  disabled,
}: SettingsSwitchProps) {
  return (
    <label className="flex items-center gap-2">
      <BaseSwitch.Root
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        className={cn(
          "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors",
          checked ? "bg-emerald-500" : "bg-zinc-300 dark:bg-zinc-600",
          disabled && "cursor-not-allowed opacity-50",
        )}
      >
        <BaseSwitch.Thumb
          className={cn(
            "inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform",
            checked ? "translate-x-4.5" : "translate-x-0.5",
          )}
        />
      </BaseSwitch.Root>
      {label && (
        <span className="text-xs text-foreground">
          {label}
        </span>
      )}
    </label>
  );
}

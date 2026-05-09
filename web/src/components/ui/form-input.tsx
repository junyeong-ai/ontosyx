"use client";

// Form primitives — three layers, each with a clear responsibility:
//
//   1. **Bare**           — `FormInput`, `FormSelect`, `FormTextarea`.
//      Native `<input>` / `<select>` / `<textarea>` with the shared
//      `formControlBase` styling. No label. Use when caller already
//      owns the label / fieldset (data grids, inline editors, custom
//      label compositions).
//
//   2. **Labelled (settings)** — `SettingsInput`, `SettingsSelect`,
//      `SettingsTextarea`. Wraps a bare control inside a `<label>`
//      with an uppercase tracking-wider caption above. Default
//      density is `"settings"`. Use in admin forms, settings rows,
//      dense list-of-config layouts.
//
//   3. **Labelled (modal/dialog)** — `<FormField>` (in form-field.tsx).
//      Wraps any control with a sentence-case medium-weight label
//      plus error / hint slots. Default density is `"default"`. Use
//      in modal forms, login, dialogs — anywhere the label should
//      read as a sentence rather than a chip.
//
// Densities map to typography / padding only. Pick by surface, not
// by control type:
//   - `"default"`   px-3 py-1.5 text-sm   modal forms, login
//   - `"settings"`  px-3 py-1.5 text-xs   admin tables, settings rows
//   - `"compact"`   px-2 py-1 text-2xs    inline editors, data-grid cells

import {
  forwardRef,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
  type SelectHTMLAttributes,
} from "react";
import { useTranslations } from "next-intl";
import { Switch as BaseSwitch } from "@base-ui/react/switch";
import type { LucideIcon } from "lucide-react";
import { Eye, EyeOff } from "lucide-react";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Form-control base
// ---------------------------------------------------------------------------
//
// Every native form control (`FormInput`, `FormSelect`, `FormTextarea`)
// shares the same border / focus / aria-invalid contract. The
// `formControlBase` function returns the canonical class string for a
// given density so a single token-update touches the whole family.
//
// Densities:
//   - `default`  px-3 py-1.5 text-sm  — modal forms, login, dialogs
//   - `settings` h-8  px-3 text-xs   — settings list-of-config rows
//   - `compact`  h-7  px-2 text-2xs  — inline editors, data-grid cells
//
// Heights are pinned to `h-N` rather than padding+line-height so a
// form control sitting beside a `<Button size="md|sm|xs">` lines up
// to the pixel — the Button primitive's heights are also `h-N`. The
// resulting matrix:
//
//   default + Button md  → both h-9 (36px)
//   settings + Button sm → both h-8 (32px)
//   compact + Button xs  → both h-7 (28px)
//
// Multi-line textareas substitute `min-h` + `py-N` so content can
// grow — `h-N` would clamp them to one line. Both axes (input vs
// textarea) share font-size + horizontal padding for register parity.

export type FormDensity = "default" | "settings" | "compact";

const densityClass: Record<FormDensity, string> = {
  default: "h-9 px-3 text-sm",
  settings: "h-8 px-3 text-xs",
  compact: "h-7 px-2 text-2xs",
};

const textareaDensityClass: Record<FormDensity, string> = {
  default: "min-h-[5rem] px-3 py-2 text-sm",
  settings: "min-h-[4rem] px-3 py-1.5 text-xs",
  compact: "min-h-[3rem] px-2 py-1 text-2xs",
};

const formControlShared =
  "w-full rounded-md border bg-surface-base text-foreground-strong " +
  "outline-none transition-colors duration-[var(--duration-quick)] " +
  "placeholder:text-foreground-subtle " +
  "border-divider focus:border-brand-foreground focus:ring-1 focus:ring-brand-foreground/40 " +
  "aria-invalid:border-danger-border aria-invalid:focus:border-danger-foreground aria-invalid:focus:ring-danger-foreground/40 " +
  "disabled:cursor-not-allowed disabled:opacity-50";

export function formControlBase(density: FormDensity = "default"): string {
  return cn(formControlShared, densityClass[density]);
}

/**
 * Textarea variant of [`formControlBase`] — substitutes `min-h` +
 * `py-N` for `h-N` so content can grow vertically. Same horizontal
 * padding + font size per density so a textarea sitting next to an
 * input reads as a single form-control family.
 */
export function formTextareaBase(density: FormDensity = "default"): string {
  return cn(formControlShared, textareaDensityClass[density]);
}

// ---------------------------------------------------------------------------
// FormInput — bare native <input>
// ---------------------------------------------------------------------------

interface FormInputProps extends InputHTMLAttributes<HTMLInputElement> {
  error?: boolean;
  density?: FormDensity;
}

export const FormInput = forwardRef<HTMLInputElement, FormInputProps>(
  ({ className, error, density = "default", ...props }, ref) => (
    <input
      ref={ref}
      aria-invalid={error || props["aria-invalid"]}
      className={cn(formControlBase(density), className)}
      {...props}
    />
  ),
);

FormInput.displayName = "FormInput";

// ---------------------------------------------------------------------------
// SearchInput — input with leading icon adornment
// ---------------------------------------------------------------------------
//
// The "search box with a magnifying-glass icon on the left" idiom
// recurs in every list / palette / explorer surface — a bare
// `<input>` inside an icon-adorned container. Without a primitive,
// every consumer hand-rolls the relative wrapper, the absolute icon
// position, the left-padding to clear the icon, and the focus ring,
// and the visual register drifts pane to pane.
//
// `SearchInput` is `FormInput` plus a `leadingIcon` slot. The icon
// is `aria-hidden` because the input's `aria-label` / placeholder
// already describes the affordance — duplicating it as an SVG
// label adds noise for screen readers without informational value.
// Trailing slot is intentionally absent; clearable search lives on
// a separate `<ClearableSearchInput>` once a real consumer needs it.

interface SearchInputProps extends InputHTMLAttributes<HTMLInputElement> {
  /**
   * Leading icon, typically `Search`. Sized automatically to
   * match `density`; the control sets the cap-height so the caller
   * doesn't need to thread `h-3 w-3` / `h-3.5 w-3.5` per surface.
   */
  leadingIcon: LucideIcon;
  density?: FormDensity;
  error?: boolean;
  /**
   * Optional trailing slot — typically a loading spinner, a close
   * button on dialog-mounted search, or a clear-x affordance. The
   * primitive sizes the right-padding to keep input text from
   * sliding under the slot. Pass `null` (default) to opt out.
   */
  trailing?: React.ReactNode;
}

const ICON_SIZE_CLASS: Record<FormDensity, string> = {
  default: "h-4 w-4",
  settings: "h-3.5 w-3.5",
  compact: "h-3 w-3",
};

const ICON_OFFSET_CLASS: Record<FormDensity, string> = {
  default: "start-3",
  settings: "start-2.5",
  compact: "start-2",
};

const INPUT_LEADING_PAD: Record<FormDensity, string> = {
  default: "ps-9",
  settings: "ps-8",
  compact: "ps-7",
};

const INPUT_TRAILING_PAD: Record<FormDensity, string> = {
  default: "pe-10",
  settings: "pe-9",
  compact: "pe-8",
};

const TRAILING_OFFSET_CLASS: Record<FormDensity, string> = {
  default: "end-2",
  settings: "end-1.5",
  compact: "end-1",
};

export const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  (
    {
      className,
      error,
      density = "default",
      leadingIcon,
      trailing,
      ...props
    },
    ref,
  ) => (
    <span className={cn("relative inline-block w-full", className)}>
      <span
        className={cn(
          "pointer-events-none absolute inset-y-0 flex items-center text-foreground-muted",
          ICON_OFFSET_CLASS[density],
        )}
        aria-hidden="true"
      >
        <DynamicIcon as={leadingIcon} className={ICON_SIZE_CLASS[density]} />
      </span>
      <input
        ref={ref}
        type={props.type ?? "search"}
        aria-invalid={error || props["aria-invalid"]}
        className={cn(
          formControlBase(density),
          INPUT_LEADING_PAD[density],
          trailing && INPUT_TRAILING_PAD[density],
        )}
        {...props}
      />
      {trailing && (
        <span
          className={cn(
            "absolute inset-y-0 flex items-center gap-1",
            TRAILING_OFFSET_CLASS[density],
          )}
        >
          {trailing}
        </span>
      )}
    </span>
  ),
);

SearchInput.displayName = "SearchInput";

// ---------------------------------------------------------------------------
// FormSelect — bare native <select> with caret affordance
// ---------------------------------------------------------------------------
//
// Native dropdowns use the OS caret, which can't be styled. We strip
// it (`appearance-none`) and overlay a tokenised chevron at the trailing
// edge so the control reads as interactive across both light and dark
// modes. The wrapping `<span>` is `inline-block` and inherits width
// from the caller — pass `className="w-..."` to constrain.
//
// New surfaces should prefer the Base UI–backed `<Select>` /
// `<SelectOption>` pair from `@/components/ui/select` so the listbox
// renders the design tokens instead of the OS chrome. `FormSelect`
// stays for the existing 36 call sites that depend on the native
// `<option>` shape; their migration to `Select` is tracked in the
// design plan.

interface FormSelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  error?: boolean;
  density?: FormDensity;
  children: ReactNode;
}

export const FormSelect = forwardRef<HTMLSelectElement, FormSelectProps>(
  ({ className, error, density = "default", children, ...props }, ref) => (
    <span className={cn("relative inline-block w-full", className)}>
      <select
        ref={ref}
        aria-invalid={error || props["aria-invalid"]}
        className={cn(formControlBase(density), "appearance-none pe-8")}
        {...props}
      >
        {children}
      </select>
      <SelectChevron />
    </span>
  ),
);

FormSelect.displayName = "FormSelect";

/**
 * Caret affordance for any element that visually implies a dropdown.
 * Used internally by `FormSelect`; also exported so Base-UI wrappers
 * (`Select` in select.tsx) can render the same icon without dragging
 * an SVG copy along.
 */
export function SelectChevron() {
  return (
    <span
      className="pointer-events-none absolute inset-y-0 end-2.5 flex items-center"
      aria-hidden="true"
    >
      <svg
        width="10"
        height="10"
        viewBox="0 0 10 10"
        fill="none"
        className="text-foreground-muted"
       aria-hidden="true">
        <path
          d="M2.5 3.75L5 6.25L7.5 3.75"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </span>
  );
}

// ---------------------------------------------------------------------------
// SecretInput — credential-aware text input with masking toggle
// ---------------------------------------------------------------------------
//
// Wraps a `FormInput` and adds an eye / eye-off button on the right
// edge. Default state is masked: the value renders as `••••` regardless
// of how it's typed, the underlying control is `type="password"`, and
// browser autofill heuristics treat it as sensitive. Pressing the eye
// reveals the literal text — useful when an operator pastes a long
// connection string and needs to confirm characters.
//
// Use for: connection strings, passwords, API keys, anything you don't
// want a casual passer-by reading off a screen.

type SecretInputProps = Omit<FormInputProps, "type">;

export const SecretInput = forwardRef<HTMLInputElement, SecretInputProps>(
  ({ className, ...props }, ref) => {
    const t = useTranslations("common.formField");
    const [revealed, setRevealed] = useState(false);
    return (
      <div className="relative">
        <FormInput
          ref={ref}
          type={revealed ? "text" : "password"}
          autoComplete="off"
          spellCheck={false}
          className={cn("font-mono pe-9", className)}
          {...props}
        />
        <button
          type="button"
          onClick={() => setRevealed((v) => !v)}
          aria-label={revealed ? t("hideSecret") : t("showSecret")}
          aria-pressed={revealed}
          className={cn(
            "absolute end-1.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded text-foreground-muted",
            "transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset hover:text-foreground",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
          )}
        >
          <DynamicIcon as={revealed ? EyeOff : Eye} className="h-3.5 w-3.5" />
        </button>
      </div>
    );
  },
);

SecretInput.displayName = "SecretInput";

// ---------------------------------------------------------------------------
// Labelled wrappers — Settings*
// ---------------------------------------------------------------------------
//
// `Settings*` are `Form*` rendered inside a `<label>` with a uppercase
// caption above the control. They default to density="settings" because
// every consumer is a settings list-of-config form. Use **implicit label
// association** (the `<label>` wraps the control) — no `id`/`htmlFor`
// thread-through needed.

const labelBase =
  "text-2xs font-semibold uppercase tracking-wider text-foreground-muted";

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

type SettingsInputProps = Omit<FormInputProps, "density"> &
  LabelledFieldProps & { density?: FormDensity };

export const SettingsInput = forwardRef<HTMLInputElement, SettingsInputProps>(
  ({ label, hideLabel, className, density = "settings", ...props }, ref) => (
    <label className="block">
      <FieldLabelText label={label} hideLabel={hideLabel} />
      <FormInput ref={ref} density={density} className={cn("mt-0.5", className)} {...props} />
    </label>
  ),
);

SettingsInput.displayName = "SettingsInput";

interface FormTextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  error?: boolean;
  density?: FormDensity;
}

export const FormTextarea = forwardRef<HTMLTextAreaElement, FormTextareaProps>(
  ({ className, error, density = "default", ...props }, ref) => (
    <textarea
      ref={ref}
      aria-invalid={error || props["aria-invalid"]}
      className={cn(formTextareaBase(density), className)}
      {...props}
    />
  ),
);

FormTextarea.displayName = "FormTextarea";

// ---------------------------------------------------------------------------
// ChatComposer — multiline textarea with an inline trailing action button
// ---------------------------------------------------------------------------
//
// Chat composers are textareas with one corner button — Send while
// idle, Stop while a stream is in flight. Without a primitive, every
// chat surface (workbench, dashboard AI, recipes runner) hand-rolls
// the auto-grow textarea + absolute-positioned button overlay, the
// pe-12 input padding to clear the button, and the disabled / streaming
// affordances. This primitive owns the layout; consumers supply the
// trailing action(s) and the value bookkeeping.
//
// Auto-grow up to `maxRows × line-height` and shrink back to one row
// on clear. The reset is handled in `onChange` rather than on a
// separate `onInput`, so React's synthetic event lifecycle stays
// consistent.

interface ChatComposerProps
  extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "rows"> {
  /** Trailing action(s). Typically a single Send / Stop button; passing a
   *  fragment of multiple buttons stacks them right-aligned. */
  trailing: React.ReactNode;
  /** Maximum visible rows before scroll. Default 8 — long enough for
   *  a paragraph, short enough not to dominate the panel. */
  maxRows?: number;
}

export const ChatComposer = forwardRef<HTMLTextAreaElement, ChatComposerProps>(
  ({ className, trailing, maxRows = 8, onChange, ...props }, ref) => {
    return (
      <div className="relative w-full">
        <textarea
          ref={ref}
          rows={1}
          onChange={(event) => {
            const el = event.currentTarget;
            // Auto-grow: reset, measure, clamp at `maxRows × line-height`.
            el.style.height = "auto";
            const lineHeightPx = parseFloat(
              window.getComputedStyle(el).lineHeight || "20",
            );
            const cap = lineHeightPx * maxRows + 24; // padding allowance
            el.style.height = `${Math.min(el.scrollHeight, cap)}px`;
            onChange?.(event);
          }}
          className={cn(
            "w-full resize-none rounded-xl border border-divider bg-surface-raised px-4 py-3 pe-12",
            "text-sm placeholder:text-foreground-muted",
            "focus:border-brand-foreground focus:bg-surface-base focus:outline-none focus:ring-2 focus:ring-brand-foreground/40",
            "disabled:opacity-50 disabled:cursor-not-allowed",
            "transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
            className,
          )}
          {...props}
        />
        <div className="absolute end-2.5 top-2 flex items-center gap-1">
          {trailing}
        </div>
      </div>
    );
  },
);

ChatComposer.displayName = "ChatComposer";

type SettingsTextareaProps = Omit<FormTextareaProps, "density"> &
  LabelledFieldProps & { density?: FormDensity };

export const SettingsTextarea = forwardRef<
  HTMLTextAreaElement,
  SettingsTextareaProps
>(({ label, hideLabel, className, density = "settings", ...props }, ref) => (
  <label className="block">
    <FieldLabelText label={label} hideLabel={hideLabel} />
    <FormTextarea
      ref={ref}
      density={density}
      className={cn("mt-0.5", className)}
      {...props}
    />
  </label>
));

SettingsTextarea.displayName = "SettingsTextarea";

type SettingsSelectProps = Omit<FormSelectProps, "density"> &
  LabelledFieldProps & { density?: FormDensity };

export const SettingsSelect = forwardRef<
  HTMLSelectElement,
  SettingsSelectProps
>(({ label, hideLabel, className, density = "settings", children, ...props }, ref) => (
  <label className="block">
    <FieldLabelText label={label} hideLabel={hideLabel} />
    <FormSelect
      ref={ref}
      density={density}
      className={cn("mt-0.5", className)}
      {...props}
    >
      {children}
    </FormSelect>
  </label>
));

SettingsSelect.displayName = "SettingsSelect";

// ---------------------------------------------------------------------------
// SettingsSwitch — Base UI Switch wrapper, label on the right
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
          "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors duration-[var(--duration-quick)]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-1",
          checked ? "bg-brand-solid" : "bg-surface-inset",
          disabled && "cursor-not-allowed opacity-50",
        )}
      >
        <BaseSwitch.Thumb
          className={cn(
            "inline-block h-3.5 w-3.5 rounded-full bg-surface-base shadow-1 transition-transform duration-[var(--duration-quick)]",
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

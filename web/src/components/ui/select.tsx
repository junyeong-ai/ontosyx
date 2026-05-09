"use client";

import { Select as BaseSelect } from "@base-ui/react/select";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Select — Base UI Select wrapper, tokenised
// ---------------------------------------------------------------------------

interface SelectProps {
  /** Controlled value */
  value?: string | null;
  /** Default (uncontrolled) value */
  defaultValue?: string | null;
  /** Called when the selected value changes */
  onValueChange?: (value: string | null) => void;
  /** Placeholder shown when no value is selected */
  placeholder?: string;
  /** Whether the select is disabled */
  disabled?: boolean;
  /** Item children (SelectOption elements) */
  children: React.ReactNode;
  /** Additional className for the trigger button */
  className?: string;
  /** Label → value map so Select.Value can display the label */
  items?: Record<string, React.ReactNode>;
  /**
   * Accessible name for the trigger when no visible `<label>`
   * is associated. Chrome menus where the eyebrow text is purely
   * visual (no form-control semantic) use this to satisfy
   * `button-name` without coercing the eyebrow into an `htmlFor`
   * / `id` pair that does not match the underlying button.
   */
  ariaLabel?: string;
  /** Forward an external label's id when the visible eyebrow is
   *  rendered as a `<span id>` outside the Select. */
  ariaLabelledBy?: string;
}

export function Select({
  value,
  defaultValue,
  onValueChange,
  placeholder,
  disabled,
  children,
  className,
  items,
  ariaLabel,
  ariaLabelledBy,
}: SelectProps) {
  return (
    <BaseSelect.Root
      value={value}
      defaultValue={defaultValue}
      onValueChange={onValueChange ? (v) => onValueChange(v) : undefined}
      disabled={disabled}
      modal={false}
      items={items}
    >
      <BaseSelect.Trigger
        aria-label={ariaLabel}
        aria-labelledby={ariaLabelledBy}
        className={cn(
          "inline-flex w-full items-center justify-between rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs text-foreground-strong transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
          "outline-none focus-visible:border-brand-foreground focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
          disabled && "cursor-not-allowed opacity-50",
          className,
        )}
      >
        <BaseSelect.Value placeholder={placeholder} />
        <BaseSelect.Icon className="ms-2 shrink-0">
          <ChevronIcon />
        </BaseSelect.Icon>
      </BaseSelect.Trigger>

      <BaseSelect.Portal>
        <BaseSelect.Positioner
          side="bottom"
          align="start"
          sideOffset={4}
        >
          <BaseSelect.Popup
            className={cn(
              // Popup container is non-interactive itself — focus is on
              // its `<Item>` children. `outline-none` here just hides
              // the browser's default focus ring on the listbox; the
              // ring lives on each item.
              "z-popover max-h-60 overflow-y-auto rounded-lg border border-divider bg-surface-base py-1 shadow-3 outline-none focus-visible:ring-0",
              "data-[starting-style]:scale-95 data-[starting-style]:opacity-0",
              "data-[ending-style]:scale-95 data-[ending-style]:opacity-0",
              "transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
            )}
          >
            {children}
          </BaseSelect.Popup>
        </BaseSelect.Positioner>
      </BaseSelect.Portal>
    </BaseSelect.Root>
  );
}

// ---------------------------------------------------------------------------
// SelectOption
// ---------------------------------------------------------------------------

interface SelectOptionProps {
  /** The value identifying this option */
  value: string;
  /** Display content */
  children: React.ReactNode;
  /** Whether this option is disabled */
  disabled?: boolean;
  /** Additional className */
  className?: string;
}

export function SelectOption({
  value,
  children,
  disabled,
  className,
}: SelectOptionProps) {
  return (
    <BaseSelect.Item
      value={value}
      disabled={disabled}
      className={cn(
        "flex cursor-default items-center px-3 py-1.5 text-xs select-none",
        "text-foreground",
        "outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
        "data-[highlighted]:bg-surface-inset",
        "data-[selected]:bg-brand-surface data-[selected]:text-brand-foreground",
        disabled && "opacity-40",
        className,
      )}
    >
      <BaseSelect.ItemText>{children}</BaseSelect.ItemText>
    </BaseSelect.Item>
  );
}

// ---------------------------------------------------------------------------
// Internal chevron icon
// ---------------------------------------------------------------------------

function ChevronIcon() {
  return (
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
  );
}

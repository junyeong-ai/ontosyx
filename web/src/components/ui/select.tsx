"use client";

import { Select as BaseSelect } from "@base-ui/react/select";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Select — Base UI Select wrapper with zinc/emerald styling
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
        className={cn(
          "inline-flex w-full items-center justify-between rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs transition-colors",
          "focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500/50",
          "dark:border-zinc-700 dark:bg-zinc-900 dark:focus:border-emerald-400 dark:focus:ring-emerald-400/50",
          disabled && "cursor-not-allowed opacity-50",
          className,
        )}
      >
        <BaseSelect.Value placeholder={placeholder} />
        <BaseSelect.Icon className="ml-2 shrink-0">
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
              "z-50 max-h-60 overflow-y-auto rounded-lg border border-zinc-200 bg-white py-1 shadow-lg outline-none",
              "dark:border-zinc-700 dark:bg-zinc-900",
              "data-[starting-style]:scale-95 data-[starting-style]:opacity-0",
              "data-[ending-style]:scale-95 data-[ending-style]:opacity-0",
              "transition-all",
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
        "flex cursor-default items-center px-3 py-1.5 text-xs outline-none select-none",
        "text-zinc-700 dark:text-zinc-300",
        "data-[highlighted]:bg-zinc-50 dark:data-[highlighted]:bg-zinc-800",
        "data-[selected]:bg-emerald-50 data-[selected]:text-emerald-700",
        "dark:data-[selected]:bg-emerald-950/30 dark:data-[selected]:text-emerald-400",
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
      className="text-zinc-500 dark:text-zinc-400"
    >
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

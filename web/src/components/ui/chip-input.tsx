"use client";

// ChipInput — multi-value text input rendered as removable chips.
// Replaces the "한 줄에 하나" textarea pattern that surfaces on
// glossary aliases / examples, rule editorial notes, value-set
// scope notes, etc. Industry pattern (Linear labels, Notion tags,
// Stripe Dashboard chips, GitHub assignees) — values stay
// individually visible, individually removable, and Enter / `,` /
// blur all commit the current draft.

import {
  useCallback,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type Ref,
} from "react";
import { useTranslations } from "next-intl";

import { cn } from "@/lib/cn";

export interface ChipInputProps<T = string> {
  values: readonly T[];
  onChange: (next: T[]) => void;
  /** Placeholder shown only when `values` is empty. */
  placeholder?: string;
  /** Disable adding/removing. */
  disabled?: boolean;
  /** Project chip label from value. Defaults to `String(item)`. */
  format?: (item: T) => string;
  /** Parse the typed text into a value. Defaults to identity (string). */
  parse?: (text: string) => T;
  /** Keys that commit the current draft. Defaults to Enter + comma. */
  commitKeys?: readonly string[];
  ariaLabel?: string;
  id?: string;
  describedBy?: string;
  tone?: "neutral" | "brand";
  monospace?: boolean;
  inputRef?: Ref<HTMLInputElement>;
}

export function ChipInput<T = string>(props: ChipInputProps<T>) {
  const {
    values,
    onChange,
    placeholder,
    disabled,
    format = (item) => String(item),
    parse = (text) => text as unknown as T,
    commitKeys = ["Enter", ","],
    ariaLabel,
    id,
    describedBy,
    tone = "neutral",
    monospace = false,
    inputRef,
  } = props;
  const t = useTranslations("forms.chipInput");
  const [draft, setDraft] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);
  const localId = useId();
  const inputId = id ?? localId;

  const commit = useCallback(
    (raw: string) => {
      const trimmed = raw.trim();
      if (!trimmed) return;
      const item = parse(trimmed);
      const label = format(item);
      const exists = values.some((v) => format(v) === label);
      if (exists) {
        setDraft("");
        return;
      }
      onChange([...values, item]);
      setDraft("");
    },
    [values, onChange, parse, format],
  );

  const removeAt = useCallback(
    (index: number) => {
      if (disabled) return;
      const next = [...values];
      next.splice(index, 1);
      onChange(next);
    },
    [values, onChange, disabled],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLInputElement>) => {
      if (commitKeys.includes(event.key)) {
        event.preventDefault();
        commit(draft);
        return;
      }
      if (event.key === "Backspace" && draft === "" && values.length > 0) {
        event.preventDefault();
        removeAt(values.length - 1);
      }
    },
    [commit, commitKeys, draft, removeAt, values.length],
  );

  const onBlur = useCallback(() => {
    if (draft.trim()) commit(draft);
  }, [commit, draft]);

  const onPaste = useCallback(
    (event: React.ClipboardEvent<HTMLInputElement>) => {
      const text = event.clipboardData.getData("text");
      // Bulk-paste support: split by newline / comma so users can
      // paste a column from a spreadsheet straight in.
      if (/[\n,]/.test(text)) {
        event.preventDefault();
        for (const piece of text.split(/[\n,]+/)) commit(piece);
      }
    },
    [commit],
  );

  const chipClass =
    tone === "brand"
      ? "bg-brand-surface text-brand-foreground"
      : "bg-surface-inset text-foreground-muted";

  return (
    <div
      ref={containerRef}
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          containerRef.current
            ?.querySelector<HTMLInputElement>("input")
            ?.focus();
        }
      }}
      className={cn(
        "flex min-h-9 flex-wrap items-center gap-1.5 rounded-md border border-divider bg-surface-base px-2 py-1.5 text-sm transition-colors duration-[var(--duration-quick)]",
        "focus-within:border-brand-border focus-within:ring-2 focus-within:ring-ring-default",
        disabled && "pointer-events-none opacity-60",
      )}
    >
      {values.map((value, index) => {
        const label = format(value);
        return (
          <span
            key={`${label}-${index}`}
            className={cn(
              "inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs",
              chipClass,
              monospace && "font-mono",
            )}
          >
            <span>{label}</span>
            <button
              type="button"
              onClick={() => removeAt(index)}
              disabled={disabled}
              aria-label={t("remove", { value: label })}
              className="rounded text-foreground-muted hover:text-foreground-strong focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring-default"
            >
              ×
            </button>
          </span>
        );
      })}
      <input
        ref={inputRef}
        id={inputId}
        type="text"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
        onPaste={onPaste}
        disabled={disabled}
        placeholder={values.length === 0 ? placeholder : undefined}
        aria-label={ariaLabel}
        aria-describedby={describedBy}
        className="min-w-[8ch] flex-1 bg-transparent text-sm placeholder:text-foreground-subtle focus:outline-none focus-visible:ring-1 focus-visible:ring-ring-default disabled:cursor-not-allowed"
      />
    </div>
  );
}

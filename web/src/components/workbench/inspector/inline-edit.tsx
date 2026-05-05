"use client";

import { useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { Check, Pencil, X } from "lucide-react";

// ---------------------------------------------------------------------------
// Inline editable field — click-to-edit text with FormInput / FormTextarea
// ---------------------------------------------------------------------------
//
// `multiline` switches the read-mode wrap behaviour and the edit-mode
// control. The default `allowEmpty = multiline` reflects the
// real-world split: a label / name (single-line) is rejected when
// blank; a description (multiline) can legitimately be cleared.
// Callers may override `allowEmpty` for unusual contracts.

export function InlineEdit({
  value,
  placeholder,
  onSave,
  className,
  inputClassName,
  multiline = false,
  allowEmpty,
}: {
  value: string;
  placeholder?: string;
  onSave: (v: string) => void;
  className?: string;
  inputClassName?: string;
  multiline?: boolean;
  allowEmpty?: boolean;
}) {
  const t = useTranslations("inspector.aria");
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const inputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const acceptEmpty = allowEmpty ?? multiline;

  useEffect(() => {
    if (!editing) return;
    const el = (multiline ? textareaRef : inputRef).current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [editing, multiline]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed === value) {
      setEditing(false);
      return;
    }
    if (!trimmed && !acceptEmpty) {
      cancel();
      return;
    }
    onSave(trimmed);
    setEditing(false);
  };

  const cancel = () => {
    setDraft(value);
    setEditing(false);
  };

  if (editing) {
    return (
      <div className={cn("flex gap-1", multiline ? "items-start" : "items-center")}>
        {multiline ? (
          <FormTextarea
            ref={textareaRef}
            density="compact"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                commit();
              }
              if (e.key === "Escape") cancel();
            }}
            rows={3}
            className={cn("min-h-[5rem] w-full resize-y border-brand-border", inputClassName)}
            placeholder={placeholder}
            aria-label={placeholder}
          />
        ) : (
          <FormInput
            ref={inputRef}
            density="compact"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit();
              if (e.key === "Escape") cancel();
            }}
            className={cn("border-brand-border", inputClassName)}
            placeholder={placeholder}
          />
        )}
        <button type="button"
          onClick={commit}
          aria-label={t("commitInline")}
          className={cn(
            "rounded text-brand-foreground hover:text-brand-foreground-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
            multiline && "mt-1",
          )}
        >
          <Check className="h-3 w-3" />
        </button>
        <button type="button"
          onClick={cancel}
          aria-label={t("cancelInline")}
          className={cn(
            "rounded text-foreground-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40",
            multiline && "mt-1",
          )}
        >
          <X className="h-3 w-3" />
        </button>
      </div>
    );
  }

  return (
    <button type="button"
      onClick={() => {
        setDraft(value);
        setEditing(true);
      }}
      className={cn(
        "group flex w-full gap-1 text-start",
        multiline ? "items-start" : "items-center",
        className,
      )}
      aria-label={t("editInline")}
    >
      <span
        className={cn(
          "flex-1",
          multiline ? "whitespace-pre-wrap break-words" : "truncate",
        )}
      >
        {value || placeholder}
      </span>
      <Pencil
        className={cn(
          "h-2.5 w-2.5 shrink-0 text-foreground-muted opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
          multiline && "mt-1",
        )}
      />
    </button>
  );
}

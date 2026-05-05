"use client";

import { useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { Check, Pencil, X } from "lucide-react";
// ---------------------------------------------------------------------------
// Inline editable field
// ---------------------------------------------------------------------------

export function InlineEdit({
  value,
  placeholder,
  onSave,
  className,
  inputClassName,
  multiline = false,
}: {
  value: string;
  placeholder?: string;
  onSave: (v: string) => void;
  className?: string;
  inputClassName?: string;
  /**
   * Long-form text. Read-mode wraps instead of truncating; edit-mode
   * renders a `<textarea>` and binds Cmd/Ctrl+Enter to commit (plain
   * Enter inserts a newline). Single-line mode keeps Enter = commit.
   */
  multiline?: boolean;
}) {
  const t = useTranslations("inspector.aria");
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Multiline edit-mode entry — focus the textarea and place the
  // caret at end-of-text. We can't use the bare `autoFocus` prop
  // because biome's a11y rule (noAutofocus) flags it; the rule
  // exists to stop autofocus on initial page load, but a button-
  // gated edit transition is the legitimate exception this hook
  // covers.
  useEffect(() => {
    if (!editing || !textareaRef.current) return;
    const ta = textareaRef.current;
    ta.focus();
    ta.setSelectionRange(ta.value.length, ta.value.length);
  }, [editing]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed !== value) {
      onSave(trimmed);
    }
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
            autoFocus
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
            "text-brand-foreground hover:text-brand-foreground-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 rounded",
            multiline && "mt-1",
          )}
        >
          <Check className="h-3 w-3" />
        </button>
        <button type="button"
          onClick={cancel}
          aria-label={t("cancelInline")}
          className={cn(
            "text-foreground-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 rounded",
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

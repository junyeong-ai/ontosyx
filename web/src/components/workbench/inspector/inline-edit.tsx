"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { FormInput } from "@/components/ui/form-input";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PencilEdit01Icon,
  Tick01Icon,
  Cancel01Icon,
} from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// Inline editable field
// ---------------------------------------------------------------------------

export function InlineEdit({
  value,
  placeholder,
  onSave,
  className,
  inputClassName,
}: {
  value: string;
  placeholder?: string;
  onSave: (v: string) => void;
  className?: string;
  inputClassName?: string;
}) {
  const t = useTranslations("inspector.aria");
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== value) {
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
      <div className="flex items-center gap-1">
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
        <button type="button"
          onClick={commit}
          aria-label={t("commitInline")}
          className="text-brand-foreground hover:text-brand-foreground-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 rounded"
        >
          <HugeiconsIcon icon={Tick01Icon} className="h-3 w-3" size="100%" />
        </button>
        <button type="button"
          onClick={cancel}
          aria-label={t("cancelInline")}
          className="text-foreground-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 rounded"
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
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
        "group flex items-center gap-1 text-start",
        className,
      )}
      aria-label={t("editInline")}
    >
      <span className="flex-1 truncate">{value || placeholder}</span>
      <HugeiconsIcon icon={PencilEdit01Icon} className="h-2.5 w-2.5 text-foreground-muted opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" size="100%" />
    </button>
  );
}

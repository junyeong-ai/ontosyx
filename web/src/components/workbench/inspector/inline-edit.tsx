"use client";

import { useState } from "react";
import { cn } from "@/lib/cn";
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
        <input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") cancel();
          }}
          className={cn(
            "w-full rounded border border-brand-border bg-surface-base px-1.5 py-0.5 text-xs outline-none",
            inputClassName,
          )}
          placeholder={placeholder}
        />
        <button onClick={commit} className="text-brand-foreground hover:text-brand-foreground">
          <HugeiconsIcon icon={Tick01Icon} className="h-3 w-3" size="100%" />
        </button>
        <button onClick={cancel} className="text-muted-foreground hover:text-foreground">
          <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
        </button>
      </div>
    );
  }

  return (
    <button
      onClick={() => {
        setDraft(value);
        setEditing(true);
      }}
      className={cn(
        "group flex items-center gap-1 text-left",
        className,
      )}
      aria-label="Click to edit"
    >
      <span className="flex-1 truncate">{value || placeholder}</span>
      <HugeiconsIcon icon={PencilEdit01Icon} className="h-2.5 w-2.5 text-foreground-muted opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" size="100%" />
    </button>
  );
}

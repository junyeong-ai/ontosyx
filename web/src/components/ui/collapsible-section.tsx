"use client";

import { useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowDown01Icon,
  ArrowRight01Icon,
} from "@hugeicons/core-free-icons";
import { cn } from "@/lib/cn";

// Generic collapsible accordion section. Used by full-screen
// surfaces with multiple heavy tiles where the operator wants to
// hide what isn't currently in focus. Distinct from the
// inspector's `Section` (shared.tsx) which is permanently open
// because the inspector lives in narrow side-rail real-estate.
export function CollapsibleSection({
  title,
  description,
  badge,
  defaultOpen = true,
  action,
  children,
}: {
  title: string;
  description?: string;
  badge?: React.ReactNode;
  defaultOpen?: boolean;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-950">
      <header
        className={cn(
          "flex items-center gap-3 px-4 py-3",
          open && "border-b border-zinc-200 dark:border-zinc-800",
        )}
      >
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className="flex flex-1 items-center gap-2 text-left"
        >
          <HugeiconsIcon
            icon={open ? ArrowDown01Icon : ArrowRight01Icon}
            className="h-3.5 w-3.5 text-muted-foreground"
            size="100%"
          />
          <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            {title}
          </h2>
          {badge}
          {description && (
            <span className="ml-2 text-[11px] text-muted-foreground">
              {description}
            </span>
          )}
        </button>
        {action && <div className="flex items-center gap-1">{action}</div>}
      </header>
      {open && <div className="px-4 py-3 text-xs">{children}</div>}
    </section>
  );
}

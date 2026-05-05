"use client";

import { useState } from "react";
import { ArrowDown, ArrowRight } from "lucide-react";
import { Heading } from "@/components/ui/heading";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
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
    <section className="rounded-lg border border-divider bg-surface-base">
      <header
        className={cn(
          "flex items-center gap-3 px-4 py-3",
          open && "border-b border-divider",
        )}
      >
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className="flex flex-1 items-center gap-2 text-start"
        >
          <DynamicIcon as={open ? ArrowDown : ArrowRight} className="h-3.5 w-3.5 text-foreground-muted" />
          <Heading level={2} size={6}>
            {title}
          </Heading>
          {badge}
          {description && (
            <span className="ms-2 text-2xs text-foreground-muted">
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

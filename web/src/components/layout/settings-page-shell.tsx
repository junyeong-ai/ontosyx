"use client";

import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { FadeIn } from "@/components/motion/fade-in";

interface SettingsPageShellProps {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
  children: ReactNode;
  /** Render the header section without a bottom divider, for pages whose
   *  content already starts with its own elevated card. Default false. */
  flushHeader?: boolean;
}

export function SettingsPageShell({
  title,
  subtitle,
  actions,
  children,
  flushHeader = false,
}: SettingsPageShellProps) {
  return (
    <div className="flex h-full flex-col">
      <header
        className={cn(
          "flex shrink-0 items-start justify-between gap-4 pb-4",
          !flushHeader && "border-b border-divider mb-6",
        )}
      >
        <div className="min-w-0">
          <h1 className="text-xl font-semibold tracking-tight text-foreground-strong">
            {title}
          </h1>
          {subtitle && (
            <p className="mt-1 text-sm text-foreground-muted">{subtitle}</p>
          )}
        </div>
        {actions && (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        )}
      </header>
      <FadeIn className="min-h-0 flex-1">{children}</FadeIn>
    </div>
  );
}

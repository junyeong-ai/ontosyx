"use client";

import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { Heading } from "@/components/ui/heading";
import { SettingsBreadcrumb } from "./settings-breadcrumb";

interface SettingsPageShellProps {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
  children: ReactNode;
  /** Render the header section without a bottom divider, for pages whose
   *  content already starts with its own elevated card. Default false. */
  flushHeader?: boolean;
}

// Standardised gutter — matches `WorkbenchPageShell.ROW_PADDING_X` so
// every page in the app reads as a single column from sidebar edge
// to gutter regardless of which shell renders it.
const PAGE_GUTTER_X = "px-4 sm:px-6 lg:px-8";

/**
 * Stacked-content page shell — visible heading + breadcrumb + body.
 * Used by `/settings/*` and the `/evaluation`, `/approvals`, `/audit`,
 * `/quality`, `/knowledge` operations surfaces.
 *
 * Self-sufficient: owns its own padding, max-width, and vertical
 * scroll, so it renders identically inside the settings layout
 * (where `<main>` is canvas-style overflow-hidden) or the workbench
 * layout (same shape). The wrapping layout supplies only the chrome
 * (sidebar + header) — never gutter.
 */
export function SettingsPageShell({
  title,
  subtitle,
  actions,
  children,
  flushHeader = false,
}: SettingsPageShellProps) {
  return (
    <div className="h-full overflow-y-auto">
      <div
        className={cn(
          "mx-auto flex w-full max-w-7xl flex-col py-6",
          PAGE_GUTTER_X,
        )}
      >
        <header
          className={cn(
            "flex shrink-0 items-start justify-between gap-4 pb-4",
            !flushHeader && "border-b border-divider mb-6",
          )}
        >
          <div className="min-w-0">
            <SettingsBreadcrumb />
            <Heading level={1} size={3}>
              {title}
            </Heading>
            {subtitle && (
              <p className="mt-1 text-sm text-foreground-muted">{subtitle}</p>
            )}
          </div>
          {actions && (
            <div className="flex shrink-0 items-center gap-2">{actions}</div>
          )}
        </header>
        <div className="min-h-0 flex-1">{children}</div>
      </div>
    </div>
  );
}

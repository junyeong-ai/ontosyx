"use client";

import type { ReactNode } from "react";
import { TabBar } from "@/components/ui/tab-bar";
import { FadeIn } from "@/components/motion/fade-in";
import type { IconSvgElement } from "@hugeicons/react";

export interface WorkbenchTab<TId extends string = string> {
  id: TId;
  label: string;
  icon?: IconSvgElement;
  badge?: number;
}

interface WorkbenchPageShellProps<TId extends string = string> {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
  tabs?: ReadonlyArray<WorkbenchTab<TId>>;
  activeTab?: TId;
  onTabChange?: (id: TId) => void;
  children: ReactNode;
}

export function WorkbenchPageShell<TId extends string = string>({
  title,
  subtitle,
  actions,
  tabs,
  activeTab,
  onTabChange,
  children,
}: WorkbenchPageShellProps<TId>) {
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 shrink-0 items-center justify-between gap-4 border-b border-divider px-4">
        <div className="flex min-w-0 items-baseline gap-3">
          <h1 className="shrink-0 text-sm font-semibold tracking-tight text-foreground-strong">
            {title}
          </h1>
          {subtitle && (
            <p className="truncate text-xs text-foreground-muted">{subtitle}</p>
          )}
        </div>
        {actions && (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        )}
      </header>

      {tabs && tabs.length > 0 && activeTab && onTabChange && (
        <div className="flex h-9 shrink-0 items-center border-b border-divider px-3">
          <TabBar
            tabs={[...tabs]}
            activeTab={activeTab}
            onTabChange={(id) => onTabChange(id as TId)}
          />
        </div>
      )}

      <FadeIn className="flex-1 overflow-auto">{children}</FadeIn>
    </div>
  );
}

"use client";

import { useId } from "react";
import { Tabs } from "@base-ui/react/tabs";
import { motion } from "motion/react";
import type { LucideIcon as IconSvgElement } from "lucide-react";
import { cn } from "@/lib/cn";
import { DynamicIcon } from "@/components/ui/dynamic-icon";

interface TabBarTab {
  id: string;
  label: string;
  icon?: IconSvgElement;
  badge?: number;
}

interface TabBarProps {
  tabs: TabBarTab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
}

export function TabBar({ tabs, activeTab, onTabChange }: TabBarProps) {
  const layoutId = useId();
  return (
    <Tabs.Root value={activeTab} onValueChange={(v) => v && onTabChange(v)}>
      <Tabs.List className="flex items-center">
        {tabs.map(({ id, label, icon, badge }) => {
          const active = id === activeTab;
          return (
            <Tabs.Tab
              key={id}
              value={id}
              className={cn(
                "relative flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-foreground/40",
                "transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                active
                  ? "text-brand-foreground"
                  : "text-foreground-muted hover:text-foreground-strong",
              )}
            >
              {icon && <DynamicIcon as={icon} className="h-3 w-3" />}
              {label}
              {badge != null && badge > 0 && (
                <span className="ms-1.5 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-warning-surface px-1 text-2xs font-bold text-warning-foreground">
                  {badge}
                </span>
              )}
              {active && (
                <motion.span
                  layoutId={layoutId}
                  className="absolute inset-x-0 -bottom-px h-0.5 bg-brand-solid"
                  transition={{
                    type: "spring",
                    bounce: 0.15,
                    duration: 0.35,
                  }}
                />
              )}
            </Tabs.Tab>
          );
        })}
      </Tabs.List>
    </Tabs.Root>
  );
}

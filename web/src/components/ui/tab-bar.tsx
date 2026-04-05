import { Tabs } from "@base-ui/react/tabs";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import { cn } from "@/lib/cn";

interface TabBarProps {
  tabs: Array<{ id: string; label: string; icon?: IconSvgElement; badge?: number }>;
  activeTab: string;
  onTabChange: (tabId: string) => void;
}

export function TabBar({ tabs, activeTab, onTabChange }: TabBarProps) {
  return (
    <Tabs.Root value={activeTab} onValueChange={(v) => v && onTabChange(v)}>
      <Tabs.List className="flex items-center">
        {tabs.map(({ id, label, icon, badge }) => (
          <Tabs.Tab
            key={id}
            value={id}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors outline-none",
              "data-[active]:border-b-2 data-[active]:border-emerald-600 data-[active]:text-emerald-600",
              "dark:data-[active]:border-emerald-400 dark:data-[active]:text-emerald-400",
              "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300",
            )}
          >
            {icon && <HugeiconsIcon icon={icon} className="h-3 w-3" size="100%" />}
            {label}
            {badge != null && badge > 0 && (
              <span className="ml-1.5 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-amber-100 px-1 text-[10px] font-bold text-amber-700 dark:bg-amber-900/40 dark:text-amber-400">
                {badge}
              </span>
            )}
          </Tabs.Tab>
        ))}
      </Tabs.List>
    </Tabs.Root>
  );
}

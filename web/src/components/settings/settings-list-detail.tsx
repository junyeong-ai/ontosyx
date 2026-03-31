"use client";

import { cn } from "@/lib/cn";
import { Skeleton } from "@/components/ui/skeleton";

interface SettingsListDetailProps<T> {
  title: string;
  items: T[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  getId: (item: T) => string;
  renderListItem: (item: T) => React.ReactNode;
  renderDetail: (item: T) => React.ReactNode;
  emptyMessage?: string;
  isLoading?: boolean;
  actions?: React.ReactNode;
  className?: string;
}

export function SettingsListDetail<T>({
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  title: _title,
  items,
  selectedId,
  onSelect,
  getId,
  renderListItem,
  renderDetail,
  emptyMessage = "No items found.",
  isLoading,
  actions,
  className,
}: SettingsListDetailProps<T>) {
  const selected = items.find((item) => getId(item) === selectedId);

  if (isLoading) {
    return (
      <div className={cn("mt-6 flex gap-6", className)}>
        <div className="w-72 shrink-0 space-y-2">
          {Array.from({ length: 5 }, (_, i) => (
            <Skeleton key={i} className="h-12 w-full" />
          ))}
        </div>
        <div className="flex-1 space-y-3">
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-3 w-32" />
          <Skeleton className="h-40 w-full" />
        </div>
      </div>
    );
  }

  return (
    <div className={cn("mt-6", className)}>
      {actions && <div className="mb-4">{actions}</div>}
      <div className="flex gap-6">
        {/* Sidebar list */}
        <div className="w-72 shrink-0 space-y-1">
          {items.length === 0 ? (
            <p className="text-sm text-zinc-400">{emptyMessage}</p>
          ) : (
            items.map((item) => {
              const id = getId(item);
              return (
                <button
                  key={id}
                  onClick={() => onSelect(id)}
                  className={cn(
                    "w-full rounded-md px-3 py-2 text-left text-sm transition-colors",
                    id === selectedId
                      ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-900/20 dark:text-emerald-400"
                      : "text-zinc-700 hover:bg-zinc-50 dark:text-zinc-300 dark:hover:bg-zinc-800",
                  )}
                >
                  {renderListItem(item)}
                </button>
              );
            })
          )}
        </div>

        {/* Detail panel */}
        <div className="flex-1">
          {selected ? (
            renderDetail(selected)
          ) : (
            <div className="text-sm text-zinc-400">
              Select an item to view details.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

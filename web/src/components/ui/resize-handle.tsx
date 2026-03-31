"use client";

import { Separator } from "react-resizable-panels";
import { cn } from "@/lib/cn";

interface ResizeHandleProps {
  orientation?: "horizontal" | "vertical";
}

export function ResizeHandle({ orientation = "horizontal" }: ResizeHandleProps) {
  const isVertical = orientation === "vertical";
  return (
    <Separator
      className={cn(
        "group relative flex items-center justify-center transition-colors",
        "hover:bg-emerald-50 dark:hover:bg-emerald-950/20",
        isVertical ? "h-2 cursor-row-resize" : "w-2 cursor-col-resize",
      )}
    >
      <div className={cn("flex items-center justify-center gap-px", isVertical ? "flex-row" : "flex-col")}>
        <div className="h-1 w-1 rounded-full bg-zinc-300 transition-colors group-hover:bg-emerald-400 dark:bg-zinc-600" />
        <div className="h-1 w-1 rounded-full bg-zinc-300 transition-colors group-hover:bg-emerald-400 dark:bg-zinc-600" />
        <div className="h-1 w-1 rounded-full bg-zinc-300 transition-colors group-hover:bg-emerald-400 dark:bg-zinc-600" />
      </div>
    </Separator>
  );
}

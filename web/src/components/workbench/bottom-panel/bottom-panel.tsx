"use client";

import { useAppStore, type DesignBottomTab } from "@/lib/store";
import { ChatPanel } from "@/components/chat/chat-panel";
import { DesignPanel } from "./design-panel";
import { QualityReportPanel } from "./quality-report-panel";
import { EmptyState } from "@/components/ui/empty-state";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { motion, AnimatePresence } from "motion/react";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import {
  Message01Icon,
  MagicWand01Icon,
  CheckListIcon,
  ArrowDown01Icon,
  ArrowUp01Icon,
} from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// Bottom panel — self-contained tabs for Workflow / Quality
// ---------------------------------------------------------------------------

/** Wrapper that reads the active project's quality report from the store. */
function QualityTab() {
  const report = useAppStore((s) => s.activeProject?.quality_report);
  if (!report) {
    return (
      <EmptyState title="No quality report available" description="Design a project first." />
    );
  }
  return (
    <div className="h-full overflow-auto p-4">
      <QualityReportPanel report={report} />
    </div>
  );
}

const tabs: { id: DesignBottomTab; label: string; icon: IconSvgElement }[] = [
  { id: "chat", label: "Chat", icon: Message01Icon },
  { id: "workflow", label: "Workflow", icon: MagicWand01Icon },
  { id: "quality", label: "Quality", icon: CheckListIcon },
];

const panelMap: Record<DesignBottomTab, React.ComponentType> = {
  chat: ChatPanel,
  workflow: DesignPanel,
  quality: QualityTab,
};

export function BottomPanel() {
  const designBottomTab = useAppStore((s) => s.designBottomTab);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);
  const isBottomPanelOpen = useAppStore((s) => s.isBottomPanelOpen);
  const toggleBottomPanel = useAppStore((s) => s.toggleBottomPanel);

  const handleTabClick = (id: DesignBottomTab) => {
    if (id === designBottomTab && isBottomPanelOpen) {
      // Active tab re-clicked → collapse (VS Code pattern)
      toggleBottomPanel();
    } else {
      // Different tab or panel closed → open + switch
      if (!isBottomPanelOpen) toggleBottomPanel();
      setDesignBottomTab(id);
    }
  };

  const ActivePanel = panelMap[designBottomTab];

  return (
    <div className="flex h-full flex-col border-t border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-950">
      {/* Tab bar — manual click handling for active-tab-toggle */}
      <div className="flex h-8 shrink-0 items-center border-b border-zinc-200 dark:border-zinc-800">
        <div className="flex items-center" role="tablist">
          {tabs.map(({ id, label, icon }) => {
            const isActive = isBottomPanelOpen && designBottomTab === id;
            return (
              <button
                key={id}
                role="tab"
                aria-selected={isActive}
                onClick={() => handleTabClick(id)}
                className={cn(
                  "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors outline-none",
                  isActive
                    ? "border-b-2 border-emerald-600 text-emerald-600 dark:border-emerald-400 dark:text-emerald-400"
                    : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300",
                )}
              >
                <HugeiconsIcon icon={icon} className="h-3 w-3" size="100%" />
                {label}
              </button>
            );
          })}
        </div>
        <div className="flex-1" />
        <Tooltip content={isBottomPanelOpen ? "Collapse panel" : "Expand panel"}>
          <button
            onClick={toggleBottomPanel}
            aria-label={isBottomPanelOpen ? "Collapse panel" : "Expand panel"}
            className="px-2 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
          >
            {isBottomPanelOpen
              ? <HugeiconsIcon icon={ArrowDown01Icon} className="h-3.5 w-3.5" size="100%" />
              : <HugeiconsIcon icon={ArrowUp01Icon} className="h-3.5 w-3.5" size="100%" />
            }
          </button>
        </Tooltip>
      </div>
      {/* Content */}
      <AnimatePresence initial={false}>
        {isBottomPanelOpen && (
          <motion.div
            key="bottom-panel-content"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
            className="flex-1 overflow-hidden"
          >
            <ActivePanel />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

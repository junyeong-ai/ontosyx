"use client";

import { useTranslations } from "next-intl";
import { useAppStore, type BottomPanelMode, type DesignBottomTab } from "@/lib/store";
import { ChatPanel } from "@/components/chat/chat-panel";
import { DesignPanel } from "./design-panel";
import { QualityReportPanel } from "./quality-report-panel";
import { EmptyState } from "@/components/ui/empty-state";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { motion, AnimatePresence } from "motion/react";
import type { LucideIcon as IconSvgElement } from "lucide-react";
import { ArrowDown, ArrowUp, MessageCircle, Wand2 } from "lucide-react";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import { ListChecks, Maximize2, Minimize2 } from "lucide-react";
// ---------------------------------------------------------------------------
// Bottom panel — self-contained tabs for Workflow / Quality
// ---------------------------------------------------------------------------

/** Wrapper that reads the active project's quality report from the store. */
function QualityTab() {
  const t = useTranslations("workbench.bottomPanel.tabs");
  const report = useAppStore((s) => s.activeProject?.quality_report);
  if (!report) {
    return (
      <EmptyState title={t("noQualityReport")} description={t("designFirst")} />
    );
  }
  return (
    <div className="h-full overflow-auto p-4">
      <QualityReportPanel report={report} />
    </div>
  );
}

const TAB_DEFS: { id: DesignBottomTab; icon: IconSvgElement }[] = [
  { id: "chat", icon: MessageCircle },
  { id: "workflow", icon: Wand2 },
  { id: "quality", icon: ListChecks },
];

const panelMap: Record<DesignBottomTab, React.ComponentType> = {
  chat: ChatPanel,
  workflow: DesignPanel,
  quality: QualityTab,
};

interface BottomPanelProps {
  /** Current snap mode — `default | tall | fullscreen`. */
  mode?: BottomPanelMode;
  /** Cycles through snap modes (default → tall → fullscreen → …). */
  onCycleMode?: () => void;
  /** Drops out of fullscreen back to `default`. */
  onExitFullscreen?: () => void;
}

export function BottomPanel({
  mode = "default",
  onCycleMode,
  onExitFullscreen,
}: BottomPanelProps = {}) {
  const t = useTranslations("workbench.bottomPanel.tabs");
  const designBottomTab = useAppStore((s) => s.designBottomTab);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);
  const isBottomPanelOpen = useAppStore((s) => s.isBottomPanelOpen);
  const toggleBottomPanel = useAppStore((s) => s.toggleBottomPanel);
  const ontology = useAppStore((s) => s.ontology);
  const isFullscreen = mode === "fullscreen";
  // Phase-aware tab filter: while no ontology has been designed yet
  // the workflow review owns the main pane (`DesignLayout` swaps the
  // canvas region for `<DesignPanel />`). Showing the same content
  // again behind a tab here would split the user's attention; hide
  // the workflow tab in that phase. Once the ontology lands, the
  // canvas takes the main pane and the workflow tab reappears.
  const reviewIsMain = ontology === null;
  const visibleTabs = reviewIsMain
    ? TAB_DEFS.filter((tab) => tab.id !== "workflow")
    : TAB_DEFS;

  // The active tab can become invalid mid-render if the user just
  // designed (workflow tab disappears). Snap to chat in that case so
  // the panel is never blank.
  const effectiveTab: DesignBottomTab =
    reviewIsMain && designBottomTab === "workflow" ? "chat" : designBottomTab;

  const handleTabClick = (id: DesignBottomTab) => {
    if (id === effectiveTab && isBottomPanelOpen) {
      // Active tab re-clicked → collapse (VS Code pattern)
      toggleBottomPanel();
    } else {
      // Different tab or panel closed → open + switch
      if (!isBottomPanelOpen) toggleBottomPanel();
      setDesignBottomTab(id);
    }
  };

  const ActivePanel = panelMap[effectiveTab];

  return (
    <div className="flex h-full flex-col border-t border-divider bg-surface-base">
      {/* Tab bar — manual click handling for active-tab-toggle */}
      <div className="flex h-8 shrink-0 items-center border-b border-divider">
        <div className="flex items-center" role="tablist">
          {visibleTabs.map(({ id, icon }) => {
            const isActive = isBottomPanelOpen && effectiveTab === id;
            return (
              <button type="button"
                key={id}
                role="tab"
                aria-selected={isActive}
                onClick={() => handleTabClick(id)}
                className={cn(
                  "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-foreground/40",
                  isActive
                    ? "border-b-2 border-brand-foreground text-brand-foreground"
                    : "text-foreground-muted hover:text-foreground-muted",
                )}
              >
                <DynamicIcon as={icon} className="h-3 w-3" />
                {t(id)}
              </button>
            );
          })}
        </div>
        <div className="flex-1" />
        {onCycleMode && (
          <Tooltip
            content={
              isFullscreen
                ? t("exitFullscreen")
                : mode === "tall"
                  ? t("enterFullscreen")
                  : t("enterTall")
            }
          >
            <button type="button"
              onClick={isFullscreen ? onExitFullscreen : onCycleMode}
              aria-label={
                isFullscreen
                  ? t("exitFullscreen")
                  : mode === "tall"
                    ? t("enterFullscreen")
                    : t("enterTall")
              }
              className="px-2 text-foreground-muted hover:text-foreground-muted"
            >
              <DynamicIcon as={isFullscreen ? Minimize2 : Maximize2} className="h-3.5 w-3.5" />
            </button>
          </Tooltip>
        )}
        <Tooltip content={isBottomPanelOpen ? t("collapsePanel") : t("expandPanel")}>
          <button type="button"
            onClick={toggleBottomPanel}
            aria-label={isBottomPanelOpen ? t("collapsePanel") : t("expandPanel")}
            className="px-2 text-foreground-muted hover:text-foreground-muted"
          >
            {isBottomPanelOpen
              ? <ArrowDown className="h-3.5 w-3.5" />
              : <ArrowUp className="h-3.5 w-3.5" />
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

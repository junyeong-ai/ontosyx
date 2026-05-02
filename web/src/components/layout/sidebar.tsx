"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { useWorkspaceMode } from "@/lib/use-workspace-mode";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import {
  AiNetworkIcon,
  Book02Icon,
  CatalogueIcon,
  ChartAnalysisIcon,
  FolderOpenIcon,
  Layers01Icon,
  Settings02Icon,
  MagicWand01Icon,
  Message01Icon,
  Search01Icon,
  DashboardSpeed01Icon,
} from "@hugeicons/core-free-icons";

function ModeLink({
  href,
  active,
  label,
  icon,
}: {
  href: string;
  active: boolean;
  label: string;
  icon: IconSvgElement;
}) {
  return (
    <Tooltip content={label} side="right">
      <Link
        href={href}
        aria-label={label}
        aria-current={active ? "page" : undefined}
        className={cn(
          "relative flex h-10 w-full items-center justify-center transition-colors",
          active
            ? "text-emerald-700 dark:text-emerald-400"
            : "text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300",
        )}
      >
        {active && (
          <span className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r-full bg-emerald-500" />
        )}
        <HugeiconsIcon icon={icon} className="h-[18px] w-[18px]" size="100%" />
      </Link>
    </Tooltip>
  );
}

function PanelToggle({
  active,
  label,
  icon,
  onClick,
}: {
  active: boolean;
  label: string;
  icon: IconSvgElement;
  onClick: () => void;
}) {
  return (
    <Tooltip content={label} side="right">
      <button
        onClick={onClick}
        aria-label={label}
        aria-pressed={active}
        className={cn(
          "flex h-9 w-full items-center justify-center transition-colors",
          active
            ? "text-zinc-600 dark:text-zinc-300"
            : "text-muted-foreground hover:text-muted-foreground dark:hover:text-muted-foreground",
        )}
      >
        <HugeiconsIcon icon={icon} className="h-4 w-4" size="100%" />
      </button>
    </Tooltip>
  );
}

export function Sidebar() {
  const t = useTranslations("chrome.sidebar");
  const workspaceMode = useWorkspaceMode();
  const pathname = usePathname();
  const onSettings = pathname?.startsWith("/settings") ?? false;
  const explorerOpen = useAppStore((s) => s.isExplorerOpen);
  const toggleExplorer = useAppStore((s) => s.toggleExplorer);
  const inspectorOpen = useAppStore((s) => s.isInspectorOpen);
  const toggleInspector = useAppStore((s) => s.toggleInspector);

  return (
    <nav aria-label={t("navAria")} className="flex h-full w-12 flex-col border-r border-zinc-200 bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900/50">
      {/* Logo */}
      <div className="flex h-11 items-center justify-center border-b border-zinc-200 dark:border-zinc-800">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-600 shadow-sm">
          <HugeiconsIcon icon={AiNetworkIcon} className="h-3.5 w-3.5 text-white" size="100%" />
        </div>
      </div>

      {/* Workspace mode switcher */}
      <nav className="flex flex-col pt-1" aria-label={t("modesAria")}>
        <ModeLink
          href="/design"
          active={!onSettings && workspaceMode === "design"}
          label={t("modeDesign")}
          icon={MagicWand01Icon}
        />
        <ModeLink
          href="/analyze"
          active={!onSettings && workspaceMode === "analyze"}
          label={t("modeAnalyze")}
          icon={Message01Icon}
        />
        <ModeLink
          href="/explore"
          active={!onSettings && workspaceMode === "explore"}
          label={t("modeExplore")}
          icon={Search01Icon}
        />
        <ModeLink
          href="/dashboard"
          active={!onSettings && workspaceMode === "dashboard"}
          label={t("modeDashboard")}
          icon={DashboardSpeed01Icon}
        />
        <ModeLink
          href="/glossary"
          active={!onSettings && workspaceMode === "glossary"}
          label={t("modeGlossary")}
          icon={Book02Icon}
        />
        <ModeLink
          href="/vocabulary"
          active={!onSettings && workspaceMode === "vocabulary"}
          label={t("modeVocabulary")}
          icon={CatalogueIcon}
        />
        <ModeLink
          href="/recipes"
          active={!onSettings && workspaceMode === "recipes"}
          label={t("modeRecipes")}
          icon={ChartAnalysisIcon}
        />
      </nav>

      {/* Separator */}
      <div className="mx-2 my-1 h-px bg-zinc-200 dark:bg-zinc-700" />

      {/* Context-sensitive panel toggles (Design mode only) */}
      {!onSettings && workspaceMode === "design" && (
        <nav className="flex flex-col" aria-label={t("panelTogglesAria")}>
          <PanelToggle
            active={explorerOpen}
            label={explorerOpen ? t("hideExplorer") : t("showExplorer")}
            icon={FolderOpenIcon}
            onClick={toggleExplorer}
          />
          <PanelToggle
            active={inspectorOpen}
            label={inspectorOpen ? t("hideInspector") : t("showInspector")}
            icon={Layers01Icon}
            onClick={toggleInspector}
          />
        </nav>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Settings */}
      <div className="flex flex-col pb-2">
        <Tooltip content={t("settingsLabel")} side="right">
          <Link
            href="/settings"
            aria-label={t("settingsLabel")}
            aria-current={onSettings ? "page" : undefined}
            className={cn(
              "flex h-10 w-full items-center justify-center transition-colors",
              onSettings
                ? "text-emerald-700 dark:text-emerald-400"
                : "text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300",
            )}
          >
            <HugeiconsIcon icon={Settings02Icon} className="h-[18px] w-[18px]" size="100%" />
          </Link>
        </Tooltip>
      </div>
    </nav>
  );
}

"use client";

import Link from "next/link";
import { useAppStore } from "@/lib/store";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import {
  AiNetworkIcon,
  FolderOpenIcon,
  Layers01Icon,
  Settings02Icon,
  MagicWand01Icon,
  Message01Icon,
  Search01Icon,
  DashboardSpeed01Icon,
} from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// Activity Bar — VS Code-style icon strip for workspace mode switching
// ---------------------------------------------------------------------------

function ModeButton({
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
          "relative flex h-10 w-full items-center justify-center transition-colors",
          active
            ? "text-emerald-600 dark:text-emerald-400"
            : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300",
        )}
      >
        {active && (
          <span className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r-full bg-emerald-500" />
        )}
        <HugeiconsIcon icon={icon} className="h-[18px] w-[18px]" size="100%" />
      </button>
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
            : "text-zinc-400 hover:text-zinc-500 dark:hover:text-zinc-400",
        )}
      >
        <HugeiconsIcon icon={icon} className="h-4 w-4" size="100%" />
      </button>
    </Tooltip>
  );
}

export function Sidebar() {
  const workspaceMode = useAppStore((s) => s.workspaceMode);
  const setWorkspaceMode = useAppStore((s) => s.setWorkspaceMode);
  const explorerOpen = useAppStore((s) => s.isExplorerOpen);
  const toggleExplorer = useAppStore((s) => s.toggleExplorer);
  const inspectorOpen = useAppStore((s) => s.isInspectorOpen);
  const toggleInspector = useAppStore((s) => s.toggleInspector);

  return (
    <aside role="navigation" aria-label="Main navigation" className="flex h-full w-12 flex-col border-r border-zinc-200 bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900/50">
      {/* Logo */}
      <div className="flex h-11 items-center justify-center border-b border-zinc-200 dark:border-zinc-800">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-600 shadow-sm">
          <HugeiconsIcon icon={AiNetworkIcon} className="h-3.5 w-3.5 text-white" size="100%" />
        </div>
      </div>

      {/* Workspace mode switcher */}
      <nav className="flex flex-col pt-1" aria-label="Workspace modes">
        <ModeButton
          active={workspaceMode === "design"}
          label="Design"
          icon={MagicWand01Icon}
          onClick={() => setWorkspaceMode("design")}
        />
        <ModeButton
          active={workspaceMode === "analyze"}
          label="Analyze"
          icon={Message01Icon}
          onClick={() => setWorkspaceMode("analyze")}
        />
        <ModeButton
          active={workspaceMode === "explore"}
          label="Explore"
          icon={Search01Icon}
          onClick={() => setWorkspaceMode("explore")}
        />
        <ModeButton
          active={workspaceMode === "dashboard"}
          label="Dashboard"
          icon={DashboardSpeed01Icon}
          onClick={() => setWorkspaceMode("dashboard")}
        />
      </nav>

      {/* Separator */}
      <div className="mx-2 my-1 h-px bg-zinc-200 dark:bg-zinc-700" />

      {/* Context-sensitive panel toggles (Design mode only) */}
      {workspaceMode === "design" && (
        <nav className="flex flex-col" aria-label="Panel toggles">
          <PanelToggle
            active={explorerOpen}
            label={explorerOpen ? "Hide Explorer" : "Show Explorer"}
            icon={FolderOpenIcon}
            onClick={toggleExplorer}
          />
          <PanelToggle
            active={inspectorOpen}
            label={inspectorOpen ? "Hide Inspector" : "Show Inspector"}
            icon={Layers01Icon}
            onClick={toggleInspector}
          />
        </nav>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Settings */}
      <div className="flex flex-col pb-2">
        <Tooltip content="Settings" side="right">
          <Link
            href="/settings"
            className="flex h-10 w-full items-center justify-center text-zinc-400 transition-colors hover:text-zinc-600 dark:hover:text-zinc-300"
            aria-label="Settings"
          >
            <HugeiconsIcon icon={Settings02Icon} className="h-[18px] w-[18px]" size="100%" />
          </Link>
        </Tooltip>
      </div>
    </aside>
  );
}

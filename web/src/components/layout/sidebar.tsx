"use client";

import { useMemo } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { useWorkspaceMode } from "@/hooks/use-workspace-mode";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import {
  listWorkbenchModes,
  workbenchModeById,
  type WorkbenchMode,
} from "@/lib/workbench-modes";
import { shortcutForRoute } from "@/lib/navigation-shortcuts";
import type { LucideIcon } from "lucide-react";
import { Settings2 } from "lucide-react";
import { FolderOpen, Layers, Network } from "lucide-react";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
function ModeLink({
  mode,
  active,
  label,
  expanded,
}: {
  mode: WorkbenchMode;
  active: boolean;
  label: string;
  expanded: boolean;
}) {
  const glyph = mode.shortcut?.glyph;
  const ariaLabel = glyph ? `${label} (${glyph})` : label;
  const link = (
    <Link
      href={mode.href}
      aria-label={ariaLabel}
      aria-current={active ? "page" : undefined}
      className={cn(
        "relative flex h-10 w-full items-center transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        expanded ? "gap-3 px-3" : "justify-center",
        active
          ? "text-brand-foreground"
          : "text-foreground-muted hover:text-foreground-muted",
      )}
    >
      {active && (
        <span className="absolute start-0 top-1.5 bottom-1.5 w-0.5 rounded-e-full bg-brand-solid" />
      )}
      <DynamicIcon as={mode.icon} className="h-[18px] w-[18px] shrink-0" />
      {expanded && (
        <span className="flex-1 truncate text-xs font-medium">{label}</span>
      )}
      {expanded && glyph && (
        <KeyboardShortcut glyph={glyph} variant="outline" />
      )}
    </Link>
  );
  if (expanded) return link;
  return (
    <Tooltip
      content={
        <span className="flex items-center gap-2">
          <span>{label}</span>
          {glyph && <KeyboardShortcut glyph={glyph} variant="outline" />}
        </span>
      }
      side="right"
    >
      {link}
    </Tooltip>
  );
}

function PanelToggle({
  active,
  label,
  icon,
  onClick,
  expanded,
}: {
  active: boolean;
  label: string;
  icon: LucideIcon;
  onClick: () => void;
  expanded: boolean;
}) {
  const button = (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-pressed={active}
      className={cn(
        "flex h-9 w-full items-center transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        expanded ? "gap-3 px-3" : "justify-center",
        active
          ? "text-foreground-muted"
          : "text-foreground-muted hover:text-foreground-muted",
      )}
    >
      <DynamicIcon as={icon} className="h-4 w-4 shrink-0" />
      {expanded && (
        <span className="flex-1 truncate text-xs">{label}</span>
      )}
    </button>
  );
  if (expanded) return button;
  return (
    <Tooltip content={label} side="right">
      {button}
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
  const sidebarMode = useAppStore((s) => s.sidebarMode);
  const expanded = sidebarMode === "expanded";

  // Hide modes that require a committed canonical ontology when the
  // workspace is greenfield. Surfaces like /mappings + /lineage only
  // render an empty-state pointing at Design mode without a canonical,
  // so the sidebar entry is noise. Visibility re-flips automatically
  // once `complete_ontology_draft` writes the first canonical version — no
  // page reload needed (TanStack invalidation re-renders the rail).
  const ontologyQuery = useWorkspaceOntology();
  const hasCanonical = !!ontologyQuery.data;
  const visibleModes = useMemo(
    () =>
      listWorkbenchModes().filter(
        (m) => !m.requiresCanonical || hasCanonical,
      ),
    [hasCanonical],
  );

  return (
    <nav
      id="sidebar"
      aria-label={t("navAria")}
      // `tabIndex={-1}` makes the skip-link target programmatically
      // focusable without adding the landmark itself to the tab cycle —
      // pressing Tab again from here lands on the first nav link, the
      // behaviour keyboard users expect from a skip link.
      tabIndex={-1}
      className={cn(
        "flex h-full flex-col border-e border-divider bg-surface-raised outline-none transition-[width] duration-[var(--duration-quick)] ease-[var(--ease-out)] focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-inset",
        expanded ? "w-48" : "w-12",
      )}
    >
      {/* Logo */}
      <div
        className={cn(
          "flex h-11 items-center border-b border-divider",
          expanded ? "gap-2 px-3" : "justify-center",
        )}
      >
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-brand-solid shadow-1">
          <Network className="h-3.5 w-3.5 text-foreground-onbrand" />
        </div>
        {expanded && (
          <span className="truncate text-sm font-semibold text-foreground-strong">
            {t("appName")}
          </span>
        )}
      </div>

      {/* Workspace mode switcher — driven by the workbench-mode
          registry (`@/lib/workbench-modes`). The default 7 modes ship
          pre-registered; plugins call `registerWorkbenchMode()` to
          add more. The sidebar, help dialog, and navigation-shortcut
          handler all read through the same registry. */}
      <nav className="flex flex-col pt-1" aria-label={t("modesAria")}>
        {visibleModes.map((m) => (
          <ModeLink
            key={m.id}
            mode={m}
            active={!onSettings && workspaceMode === m.id}
            label={t(m.labelKey)}
            expanded={expanded}
          />
        ))}
      </nav>

      {/* Separator */}
      <div className="mx-2 my-1 h-px bg-surface-inset" />

      {/* Context-sensitive panel toggles — opt-in via the registry's
          `hasPanelToggles` flag. Today only `design` has them; any
          future mode that grows an explorer / inspector pair flips
          the flag and inherits the toggles for free. */}
      {!onSettings && workbenchModeById(workspaceMode)?.hasPanelToggles && (
        <nav className="flex flex-col" aria-label={t("panelTogglesAria")}>
          <PanelToggle
            active={explorerOpen}
            label={explorerOpen ? t("hideExplorer") : t("showExplorer")}
            icon={FolderOpen}
            onClick={toggleExplorer}
            expanded={expanded}
          />
          <PanelToggle
            active={inspectorOpen}
            label={inspectorOpen ? t("hideInspector") : t("showInspector")}
            icon={Layers}
            onClick={toggleInspector}
            expanded={expanded}
          />
        </nav>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Settings — chrome footer, not a workbench mode, but renders
          with the same ModeLink style so the visual register matches. */}
      <div className="flex flex-col pb-2">
        <ModeLink
          mode={{
            id: "settings",
            labelKey: "settingsLabel",
            icon: Settings2,
            href: "/settings",
            shortcut: shortcutForRoute("settings"),
          }}
          active={onSettings}
          label={t("settingsLabel")}
          expanded={expanded}
        />
      </div>
    </nav>
  );
}

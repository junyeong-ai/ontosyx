"use client";

// GlobalCommandSource — registers the cross-app navigation + view
// toggles into the unified command registry. Mounted once inside
// the workbench layout; unmounts (and removes the source) when the
// user leaves the workbench area.
//
// The component renders nothing — it's a registration vehicle. The
// commands themselves are resolved at thunk time so visibility
// predicates always see the current store state.

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";

import {
  type Command,
  type CommandContext,
  type CommandSource,
  commandRegistry,
} from "@/lib/command-registry";
import { usePlugin } from "@/lib/plugins/use-plugin";
import { useAppStore } from "@/lib/store";
import {
  NAVIGATION_SHORTCUTS,
  type NavigationRoute,
} from "@/lib/navigation-shortcuts";

const NAV_LABEL_KEY: Record<NavigationRoute, string> = {
  design: "navigate-design",
  analyze: "navigate-analyze",
  explore: "navigate-explore",
  dashboard: "navigate-dashboard",
  glossary: "navigate-glossary",
  vocabulary: "navigate-vocabulary",
  mappings: "navigate-mappings",
  lineage: "navigate-lineage",
  branches: "navigate-branches",
  recipes: "navigate-recipes",
  settings: "navigate-settings",
};

export function GlobalCommandSource() {
  const t = useTranslations("commandPalette.commands");
  const tGroups = useTranslations("commandPalette.groups");

  const buildCommands = useCallback((): Command[] => {
    const store = useAppStore.getState();
    const cmds: Command[] = [
      {
        id: "search-entities",
        label: t("search-entities.label"),
        shortcut: { mac: "⌘K", other: "Ctrl+K" },
        keywords: ["search", "find", "검색"],
        execute: ({ store: s }: CommandContext) => {
          s.setState({ isSearchOpen: true });
        },
      },
      ...NAVIGATION_SHORTCUTS.map((nav): Command => ({
        id: NAV_LABEL_KEY[nav.route],
        label: t(`${NAV_LABEL_KEY[nav.route]}.label`),
        shortcut: { mac: nav.glyph, other: nav.glyph },
        execute: ({ router }) => router.push(nav.href),
      })),
      {
        id: "toggle-explorer",
        label: t("toggle-explorer.label"),
        execute: ({ store: s }) => {
          s.getState().toggleExplorer();
        },
      },
      {
        id: "toggle-inspector",
        label: t("toggle-inspector.label"),
        execute: ({ store: s }) => {
          s.getState().toggleInspector();
        },
      },
      {
        id: "toggle-bottom-panel",
        label: t("toggle-bottom-panel.label"),
        execute: ({ store: s }) => {
          s.getState().toggleBottomPanel();
        },
      },
      {
        id: "cycle-bottom-panel-mode",
        label: t("cycle-bottom-panel-mode.label"),
        shortcut: { mac: "⌘\\", other: "Ctrl+\\" },
        execute: ({ store: s }) => {
          s.getState().cycleBottomPanelMode();
        },
      },
    ];
    if (store.bottomPanelMode !== "fullscreen") {
      cmds.push({
        id: "panel-mode-fullscreen",
        label: t("panel-mode-fullscreen.label"),
        shortcut: { mac: "⌘⇧\\", other: "Ctrl+Shift+\\" },
        execute: ({ store: s }) => {
          s.getState().setBottomPanelMode("fullscreen");
        },
      });
    }
    if (store.bottomPanelMode !== "default") {
      cmds.push({
        id: "panel-mode-default",
        label: t("panel-mode-default.label"),
        shortcut: { mac: "Esc", other: "Esc" },
        execute: ({ store: s }) => {
          s.getState().setBottomPanelMode("default");
        },
      });
    }
    return cmds;
  }, [t]);

  const source = useMemo<CommandSource>(
    () => ({
      id: "global",
      groupLabel: tGroups("global"),
      order: 0,
      commands: buildCommands,
    }),
    [tGroups, buildCommands],
  );

  usePlugin(commandRegistry, source);
  return null;
}

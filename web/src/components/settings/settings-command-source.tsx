"use client";

// SettingsCommandSource — registers every visible settings page as
// a command in the unified registry while the user is inside the
// settings layout. Unmounting the layout (navigating to workbench)
// removes the source, so the palette doesn't surface settings pages
// from unrelated contexts.

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";

import { useAuth } from "@/hooks/use-auth";
import {
  type Command,
  type CommandSource,
  commandRegistry,
} from "@/lib/command-registry";
import { usePlugin } from "@/lib/plugins/use-plugin";
import { SETTINGS_NAV_GROUPS } from "@/lib/constants/settings-nav";

interface IndexEntry {
  href: string;
  itemKey: string;
  groupKey: string;
  authOnly?: boolean;
  adminOnly?: boolean;
}

function buildIndex(): IndexEntry[] {
  const entries: IndexEntry[] = [];
  for (const group of SETTINGS_NAV_GROUPS) {
    if (group.href && group.items.length === 0) {
      entries.push({
        href: group.href,
        itemKey: group.titleKey,
        groupKey: group.titleKey,
      });
      continue;
    }
    for (const item of group.items) {
      entries.push({
        href: item.href,
        itemKey: item.labelKey,
        groupKey: group.titleKey,
        authOnly: item.authOnly,
        adminOnly: item.adminOnly,
      });
    }
  }
  return entries;
}

const FULL_INDEX = buildIndex();

export function SettingsCommandSource() {
  const tSidebar = useTranslations("settings.chrome.sidebar");
  const tGroups = useTranslations("commandPalette.groups");
  const { authEnabled, isAdmin } = useAuth();

  const buildCommands = useCallback((): Command[] => {
    return FULL_INDEX.filter(
      (e) => (!e.authOnly || authEnabled) && (!e.adminOnly || isAdmin),
    ).map(
      (entry): Command => ({
        id: entry.href,
        label: tSidebar(entry.itemKey),
        keywords: [tSidebar(entry.groupKey)],
        execute: ({ router }) => router.push(entry.href),
      }),
    );
  }, [authEnabled, isAdmin, tSidebar]);

  const source = useMemo<CommandSource>(
    () => ({
      id: "settings",
      groupLabel: tGroups("settings"),
      order: 10,
      commands: buildCommands,
    }),
    [tGroups, buildCommands],
  );

  usePlugin(commandRegistry, source);
  return null;
}

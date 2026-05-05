"use client";

// CanvasCommandSource — registers ontology-canvas operations into
// the unified command registry while the canvas is mounted. Schema
// edits, layout/zoom operations, export actions all surface in the
// global ⌘K palette under the "Canvas" group; unmounting the
// canvas (project switch, navigating away from /design) removes
// the entries.

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";
import { useReactFlow } from "@xyflow/react";

import { usePrompt } from "@/components/providers/prompt-provider";
import {
  type Command,
  type CommandSource,
  commandRegistry,
} from "@/lib/command-registry";
import { usePlugin } from "@/lib/plugins/use-plugin";
import { useAppStore } from "@/lib/store";

export interface CanvasCommandActions {
  handleSave: () => void;
  deleteSelected: () => void;
  runAutoLayout: () => void;
  selectAllNodes: () => void;
  deselectAll: () => void;
  exportPng: () => void;
  exportSvg: () => void;
}

export function CanvasCommandSource({ actions }: { actions: CanvasCommandActions }) {
  const t = useTranslations("workbench.canvas.commands");
  const tGroups = useTranslations("commandPalette.groups");
  const { fitView, zoomIn, zoomOut } = useReactFlow();
  const prompt = usePrompt();

  const buildCommands = useCallback((): Command[] => {
    return [
      {
        id: "undo",
        label: t("undo"),
        shortcut: { mac: "⌘Z", other: "Ctrl+Z" },
        execute: () => useAppStore.getState().undo(),
      },
      {
        id: "redo",
        label: t("redo"),
        shortcut: { mac: "⌘⇧Z", other: "Ctrl+Shift+Z" },
        execute: () => useAppStore.getState().redo(),
      },
      {
        id: "save",
        label: t("save"),
        shortcut: { mac: "⌘S", other: "Ctrl+S" },
        execute: actions.handleSave,
      },
      {
        id: "auto-layout",
        label: t("autoLayout"),
        execute: actions.runAutoLayout,
      },
      {
        id: "fit-view",
        label: t("fitView"),
        shortcut: { mac: "⌘0", other: "Ctrl+0" },
        execute: () => fitView({ padding: 0.15, duration: 300 }),
      },
      {
        id: "export-png",
        label: t("exportPng"),
        execute: actions.exportPng,
      },
      {
        id: "export-svg",
        label: t("exportSvg"),
        execute: actions.exportSvg,
      },
      {
        id: "add-node",
        label: t("addNode"),
        execute: async () => {
          const label = await prompt({
            title: t("addNodePrompt.title"),
            description: t("addNodePrompt.description"),
            placeholder: t("addNodePrompt.placeholder"),
            confirmLabel: t("addNodePrompt.confirm"),
          });
          if (label?.trim()) {
            useAppStore.getState().applyCommand({
              op: "add_node",
              id: crypto.randomUUID(),
              label: label.trim(),
            });
          }
        },
      },
      {
        id: "delete-selected",
        label: t("deleteSelected"),
        shortcut: { mac: "Del", other: "Del" },
        execute: actions.deleteSelected,
      },
      {
        id: "select-all",
        label: t("selectAll"),
        shortcut: { mac: "⌘A", other: "Ctrl+A" },
        execute: actions.selectAllNodes,
      },
      {
        id: "deselect-all",
        label: t("deselectAll"),
        shortcut: { mac: "Esc", other: "Esc" },
        execute: actions.deselectAll,
      },
      {
        id: "zoom-in",
        label: t("zoomIn"),
        shortcut: { mac: "⌘+", other: "Ctrl++" },
        execute: () => zoomIn(),
      },
      {
        id: "zoom-out",
        label: t("zoomOut"),
        shortcut: { mac: "⌘-", other: "Ctrl+-" },
        execute: () => zoomOut(),
      },
    ];
  }, [t, actions, fitView, zoomIn, zoomOut, prompt]);

  const source = useMemo<CommandSource>(
    () => ({
      id: "canvas",
      groupLabel: tGroups("canvas"),
      order: 20,
      commands: buildCommands,
    }),
    [tGroups, buildCommands],
  );

  usePlugin(commandRegistry, source);
  return null;
}

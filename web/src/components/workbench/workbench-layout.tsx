"use client";

import { useAppStore } from "@/lib/store";
import { DesignLayout } from "./design-layout";
import { AnalyzeLayout } from "./analyze-layout";
import { ExploreLayout } from "./explore-layout";
import { DashboardLayout } from "./dashboard-layout";
import { KeyboardShortcutsDialog } from "@/components/ui/keyboard-shortcuts";

// ---------------------------------------------------------------------------
// Workbench — dispatches to mode-specific layouts
// ---------------------------------------------------------------------------

export function WorkbenchLayout() {
  const mode = useAppStore((s) => s.workspaceMode);

  let content;
  switch (mode) {
    case "design":
      content = <DesignLayout />;
      break;
    case "analyze":
      content = <AnalyzeLayout />;
      break;
    case "explore":
      content = <ExploreLayout />;
      break;
    case "dashboard":
      content = <DashboardLayout />;
      break;
  }

  return (
    <div className="h-full overflow-hidden">
      {content}
      <KeyboardShortcutsDialog />
    </div>
  );
}

"use client";

import { useTranslations } from "next-intl";

// Workbench routes that render a canvas / chat / explore surface
// instead of a `WorkbenchPageShell`. Each one needs an `<h1>` for
// axe's `page-has-heading-one` rule, but the visible chrome is the
// canvas itself, so the heading is screen-reader-only.
//
// Keys mirror the sidebar nav labels (`chrome.sidebar.modeXxx`) so
// the announced heading matches what the user clicked to get here.
export type WorkbenchRoute =
  | "design"
  | "analyze"
  | "explore"
  | "dashboard";

const SIDEBAR_KEY: Record<WorkbenchRoute, string> = {
  design: "modeDesign",
  analyze: "modeAnalyze",
  explore: "modeExplore",
  dashboard: "modeDashboard",
};

export function RouteHeading({ route }: { route: WorkbenchRoute }) {
  const t = useTranslations("chrome.sidebar");
  return <h1 className="sr-only">{t(SIDEBAR_KEY[route])}</h1>;
}

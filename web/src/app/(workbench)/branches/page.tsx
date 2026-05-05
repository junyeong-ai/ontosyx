"use client";

import { useTranslations } from "next-intl";

import { BranchingTree } from "@/components/workbench/branches/branching-tree";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";

export default function BranchesPage() {
  const t = useTranslations("workbench.branches");
  return (
    <WorkbenchPageShell title={t("pageTitle")}>
      <BranchingTree />
    </WorkbenchPageShell>
  );
}

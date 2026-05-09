"use client";

// Consolidated Audit page — `사용자` (admin actions on platform
// resources) and `PROV-O` (fact-level provenance of every scan, rule
// evaluation, action, etc.) live as facets behind a single tabbed
// surface. Different abstraction levels of "audit" used to be two
// separate sidebar items; the tabbed shell preserves discovery while
// trimming the governance group from 10 → 4 items.

import { useTranslations } from "next-intl";
import { useRouter, useSearchParams } from "next/navigation";

import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { UserAuditFacet } from "./_facets/user-audit-facet";
import { ProvenanceAuditFacet } from "./_facets/provenance-audit-facet";

const TABS = ["user", "provenance"] as const;
type AuditTab = (typeof TABS)[number];

function isAuditTab(value: string | null): value is AuditTab {
  return value !== null && (TABS as readonly string[]).includes(value);
}

export default function AuditPage() {
  const t = useTranslations("settings.governance.audit");
  const router = useRouter();
  const params = useSearchParams();

  const tabParam = params.get("tab");
  const activeTab: AuditTab = isAuditTab(tabParam) ? tabParam : "user";

  const handleTabChange = (next: AuditTab) => {
    router.replace(next === "user" ? "/audit" : `/audit?tab=${next}`, {
      scroll: false,
    });
  };

  return (
    <WorkbenchPageShell<AuditTab>
      title={t("pageTitle")}
      tabs={[
        { id: "user", label: t("tab.user") },
        { id: "provenance", label: t("tab.provenance") },
      ]}
      activeTab={activeTab}
      onTabChange={handleTabChange}
    >
      {activeTab === "user" ? <UserAuditFacet /> : <ProvenanceAuditFacet />}
    </WorkbenchPageShell>
  );
}

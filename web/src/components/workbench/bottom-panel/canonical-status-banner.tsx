"use client";

import { useTranslations } from "next-intl";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";

/**
 * Reads the workspace's singleton canonical ontology and renders a
 * one-line status row above the create-draft flow. Communicates the
 * draft-branches-off-canonical relationship at a glance:
 *
 * - Greenfield (no canonical) → "이 초안이 워크스페이스의 첫 대표 온톨로지이
 *   됩니다" so the operator knows v1 is what they're about to author.
 * - Existing canonical → "현재 대표 온톨로지: v{N}" so the operator knows
 *   the next completion lands at v{N+1}.
 *
 * The component is intentionally text-only and pre-translation —
 * styling stays consistent with the workspace pill on the header.
 */
export function CanonicalStatusBanner() {
  const t = useTranslations("workbench.bottomPanel.canonicalStatus");
  const ontologyQuery = useWorkspaceOntology();

  if (ontologyQuery.isLoading) return null;

  const canonical = ontologyQuery.data ?? null;
  const versionLabel = canonical?.current_version?.version ?? null;

  return (
    <div className="rounded-md border border-divider bg-surface-inset/40 px-3 py-2 text-xs text-foreground-muted">
      {canonical && versionLabel ? (
        <span>
          {t("withVersion", { version: versionLabel })}
        </span>
      ) : (
        <span>{t("greenfield")}</span>
      )}
    </div>
  );
}

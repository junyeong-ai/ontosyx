"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";

import { arr } from "@/lib/ir-collections";
import type { EdgeTypeDef, NodeTypeDef, OntologyIR } from "@/types/api";

// Wire-shape `RuleDef` from the OpenAPI generator. Use the IR field
// type directly so transition.from (string | null | undefined) keeps
// its nullable wire shape — the edit-ops local mirror narrows it.
type RuleDef = NonNullable<OntologyIR["rules"]>[number];
type RuleSeverity = NonNullable<RuleDef["severity"]>;

// RulesFacet — reverse-pointer panel that lists every `RuleDef`
// whose `kind.target_*` matches the currently-selected entity. The
// inspector renders this alongside Definition / Sample / Lineage /
// Quality / Changelog so the modeller can see "what invariants
// guard this entity?" without leaving the canvas.

interface RulesFacetProps {
  ontology: OntologyIR;
  entity: NodeTypeDef | EdgeTypeDef;
  kind: "node" | "edge";
}

export function RulesFacet({ ontology, entity, kind }: RulesFacetProps) {
  const t = useTranslations("inspector.rules");
  const rules = collectRulesForEntity(ontology, entity, kind);

  if (rules.length === 0) {
    return (
      <p className="text-2xs text-foreground-muted">{t("none")}</p>
    );
  }

  return (
    <ul className="flex flex-col gap-2">
      {rules.map((rule) => (
        <li
          key={rule.id}
          className="rounded-md border border-divider-soft bg-surface-base p-2.5"
        >
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0 flex-1">
              <p className="font-medium text-foreground-strong">
                {rule.name?.default ?? rule.id}
              </p>
              {rule.description?.default && (
                <p className="mt-0.5 text-2xs text-foreground-muted">
                  {rule.description.default}
                </p>
              )}
              <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-2xs">
                <SeverityBadge severity={rule.severity ?? "violation"} />
                <KindBadge ruleKind={rule.kind?.kind} />
                <ConstraintCount
                  count={arr(rule.constraints).length}
                  label={t("constraintCount", {
                    count: arr(rule.constraints).length,
                  })}
                />
                {rule.enforcement && (
                  <span className="rounded bg-surface-inset px-1.5 py-0.5 font-medium text-foreground-muted">
                    {t(`enforcement.${rule.enforcement}`)}
                  </span>
                )}
              </div>
            </div>
            <Link
              href={`/settings/quality?tab=rules&id=${encodeURIComponent(rule.id)}`}
              className="shrink-0 rounded px-2 py-1 text-2xs font-medium text-brand-foreground hover:bg-brand-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring-default"
            >
              {t("openLink")}
            </Link>
          </div>
        </li>
      ))}
    </ul>
  );
}

function collectRulesForEntity(
  ontology: OntologyIR,
  entity: NodeTypeDef | EdgeTypeDef,
  kind: "node" | "edge",
): RuleDef[] {
  const out: RuleDef[] = [];
  for (const rule of arr(ontology.rules)) {
    const target = rule.kind;
    if (!target) continue;
    if (kind === "node") {
      if (
        ("target_node_type_id" in target &&
          target.target_node_type_id === entity.id) ||
        ("state_property_id" in target &&
          target.target_node_type_id === entity.id)
      ) {
        out.push(rule);
      }
    } else if (
      "target_edge_type_id" in target &&
      target.target_edge_type_id === entity.id
    ) {
      out.push(rule);
    }
  }
  return out;
}

function SeverityBadge({
  severity,
}: {
  severity: RuleSeverity;
}) {
  const t = useTranslations("inspector.rules.severity");
  const styles: Record<string, string> = {
    violation: "bg-danger-surface text-danger-foreground",
    warning: "bg-warning-surface text-warning-foreground",
    info: "bg-info-surface text-info-foreground",
  };
  return (
    <span
      className={`rounded px-1.5 py-0.5 font-bold uppercase ${styles[severity] ?? ""}`}
    >
      {t(severity)}
    </span>
  );
}

function KindBadge({ ruleKind }: { ruleKind: string | undefined }) {
  const t = useTranslations("inspector.rules.kind");
  if (!ruleKind) return null;
  return (
    <span className="rounded bg-concept-surface px-1.5 py-0.5 font-medium text-concept-foreground">
      {t(ruleKind)}
    </span>
  );
}

function ConstraintCount({
  count,
  label,
}: {
  count: number;
  label: string;
}) {
  if (count === 0) return null;
  return (
    <span className="rounded bg-surface-inset px-1.5 py-0.5 font-mono text-foreground-muted">
      {label}
    </span>
  );
}

"use client";

import { useCallback } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import { ArrowLeft, CheckCheck } from "lucide-react";
import { useAppStore } from "@/lib/store";
import { selectStateOntology } from "@/lib/store/selectors";
import { arr } from "@/lib/ir-collections";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import { Tooltip } from "@/components/ui/tooltip";
import { defaultText } from "@/lib/locale/localize";
import type { NodeTypeDef, OntologyIR, QualityGap } from "@/types/api";
import type { SchemaEntityRef } from "@/lib/api/dependencies";
import { gapTouchesEntity } from "@/lib/quality-utils";

import { InlineEdit } from "@/components/workbench/inspector/inline-edit";
import { DefinitionFacet } from "@/components/workbench/inspector/facets/definition-facet";
import { PropertiesFacet } from "@/components/workbench/inspector/facets/properties-facet";
import { SamplesFacet } from "@/components/workbench/inspector/facets/samples-facet";
import { ConstraintsFacet } from "@/components/workbench/inspector/facets/constraints-facet";
import { MappingsFacet } from "@/components/workbench/inspector/facets/mappings-facet";
import { LineageFacet } from "@/components/workbench/inspector/facets/lineage-facet";
import { QualityFacet } from "@/components/workbench/inspector/facets/quality-facet";
import { ChangeLogFacet } from "@/components/workbench/inspector/facets/change-log-facet";

// Stable empty reference so the Zustand selector returns the same
// array across renders when no quality report is loaded — keeps
// `useAppStore` from re-firing and avoids needless re-renders.
const EMPTY_GAPS: readonly QualityGap[] = [];

/**
 * Entity-centric domain context page. Eight canonical facets —
 * Definition, Properties, Samples, Constraints, Mappings, Lineage,
 * Quality, Change log — surface every aspect a modeller might shape
 * for one business concept. Each facet is a shared component; the
 * canvas inspector renders the same set under a tabbed layout so
 * behaviour stays in lockstep.
 */
export function DomainContextPage({ nodeId }: { nodeId: string }) {
  const t = useTranslations("workbench.types.detail");
  const ontology = useAppStore(selectStateOntology);

  if (!ontology) {
    return <EmptyShell message={t("noOntology")} />;
  }

  const node = arr(ontology.node_types).find((n) => n.id === nodeId);
  if (!node) {
    return <EmptyShell message={t("nodeNotFound", { id: nodeId })} />;
  }

  return <NodeView ontology={ontology} node={node} />;
}

function NodeView({
  ontology,
  node,
}: {
  ontology: OntologyIR;
  node: NodeTypeDef;
}) {
  const t = useTranslations("workbench.types.detail");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const allGaps = useAppStore(
    (s) => s.activeOntologyDraft?.quality_report?.gaps ?? EMPTY_GAPS,
  );

  const propertyCount = arr(node.properties).length;
  const constraintCount = arr(node.constraints).length;
  const entityRef: SchemaEntityRef = { kind: "node_type", id: node.id };

  const handleRename = useCallback(
    (newLabel: string) => {
      applyCommand({
        op: "rename_node",
        node_id: node.id,
        new_label: newLabel,
      });
    },
    [applyCommand, node.id],
  );

  const readiness = evaluateReadiness(node, ontology);
  const readinessPassed = readiness.filter((r) => r.passed).length;

  // Quality gaps are read off the active project's persisted quality
  // report (the same source the canvas overlay consumes). Filtering
  // by entity here keeps the facet identical to the inspector's
  // selection-scoped view.
  const gaps: QualityGap[] = allGaps.filter((g) =>
    gapTouchesEntity(g, "node", node.id),
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <PageHeader
        label={node.label}
        backLabel={t("backToCanvas")}
        validateLabel={t("validateCompleteness")}
        readiness={readiness}
        readinessPassed={readinessPassed}
        onRename={handleRename}
      />
      <div className="flex-1 overflow-auto">
        <div className="mx-auto max-w-5xl space-y-3 px-6 py-6">
          <CollapsibleSection
            title={t("sections.definition.title")}
            description={t("sections.definition.subtitle")}
            badge={node.concept_id ? <CountBadge count={1} /> : undefined}
          >
            <DefinitionFacet ontology={ontology} entity={node} kind="node" />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.properties.title")}
            description={t("sections.properties.subtitle")}
            badge={<CountBadge count={propertyCount} />}
          >
            <PropertiesFacet ontology={ontology} entity={node} kind="node" />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.samples.title")}
            description={t("sections.samples.subtitle")}
          >
            <SamplesFacet node={node} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.constraints.title")}
            description={t("sections.constraints.subtitle")}
            badge={
              constraintCount > 0 ? (
                <CountBadge count={constraintCount} />
              ) : undefined
            }
            defaultOpen={false}
          >
            <ConstraintsFacet node={node} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.mappings.title")}
            description={t("sections.mappings.subtitle")}
            defaultOpen={false}
          >
            <MappingsFacet node={node} ontology={ontology} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.lineage.title")}
            description={t("sections.lineage.subtitle")}
            defaultOpen={false}
          >
            <LineageFacet ontology={ontology} entityRef={entityRef} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.quality.title")}
            description={t("sections.quality.subtitle")}
            defaultOpen={false}
          >
            <QualityFacet gaps={gaps} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.changelog.title")}
            description={t("sections.changelog.subtitle")}
            defaultOpen={false}
          >
            <ChangeLogFacet ontology={ontology} entity={node} kind="node" />
          </CollapsibleSection>
        </div>
      </div>
    </div>
  );
}

interface ReadinessCheck {
  id: string;
  passed: boolean;
}

function evaluateReadiness(
  node: NodeTypeDef,
  ontology: OntologyIR,
): ReadinessCheck[] {
  const description = defaultText(node.description).trim();
  const hasMapping = arr(ontology.object_mappings).some(
    (m) => m.node_type_id === node.id,
  );
  return [
    { id: "description", passed: description.length > 0 },
    { id: "concept", passed: !!node.concept_id },
    { id: "properties", passed: arr(node.properties).length > 0 },
    { id: "mapping", passed: hasMapping },
    { id: "sourceLineage", passed: !!node.source_lineage?.table },
  ];
}

function PageHeader({
  label,
  backLabel,
  validateLabel,
  readiness,
  readinessPassed,
  onRename,
}: {
  label: string;
  backLabel: string;
  validateLabel: string;
  readiness: readonly ReadinessCheck[];
  readinessPassed: number;
  onRename: (next: string) => void;
}) {
  const t = useTranslations("workbench.types.detail.readiness");
  const tHeader = useTranslations("workbench.types.detail.header");
  const total = readiness.length;
  const allPassed = readinessPassed === total;
  return (
    <header className="flex shrink-0 items-center gap-3 border-b border-divider bg-surface-base px-6 py-3">
      <Link
        href="/design"
        aria-label={backLabel}
        className="rounded p-1 text-foreground-muted hover:bg-surface-inset hover:text-foreground-strong"
      >
        <ArrowLeft className="h-4 w-4" />
      </Link>
      <span className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-bold uppercase text-brand-foreground-strong">
        {tHeader("nodeBadge")}
      </span>
      <div className="flex flex-1 flex-col">
        <InlineEdit
          value={label}
          onSave={onRename}
          className="text-sm font-semibold text-foreground-strong"
        />
      </div>
      <Tooltip
        content={
          <ul className="space-y-0.5 text-2xs">
            {readiness.map((r) => (
              <li key={r.id} className="flex items-center gap-2">
                <span className={r.passed ? "text-brand-foreground" : "text-danger-foreground"}>
                  {r.passed ? "✓" : "✗"}
                </span>
                <span>{t(`checks.${r.id}`)}</span>
              </li>
            ))}
          </ul>
        }
      >
        <span
          className={
            "inline-flex items-center gap-1.5 rounded border px-3 py-1.5 text-xs " +
            (allPassed
              ? "border-brand-border bg-brand-surface text-brand-foreground-strong"
              : "border-warning-border bg-warning-surface text-warning-foreground")
          }
          aria-label={validateLabel}
        >
          <CheckCheck className="h-3 w-3" />
          {t("summary", { passed: readinessPassed, total })}
        </span>
      </Tooltip>
    </header>
  );
}

function EmptyShell({ message }: { message: string }) {
  return (
    <div className="flex h-full items-center justify-center px-6 py-12 text-sm text-foreground-muted">
      <div className="max-w-md text-center">{message}</div>
    </div>
  );
}

function CountBadge({ count }: { count: number }) {
  return (
    <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground-muted">
      {count}
    </span>
  );
}

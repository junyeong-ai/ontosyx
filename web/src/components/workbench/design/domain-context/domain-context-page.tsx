"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { ArrowLeft01Icon, Tick02Icon } from "@hugeicons/core-free-icons";

import { useAppStore } from "@/lib/store";
import { selectStateOntology } from "@/lib/store/selectors";
import { arr } from "@/lib/ir-collections";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/lib/use-locale-chain";
import { CollapsibleSection } from "@/components/ui/collapsible-section";

/**
 * Domain Context page for one NodeType. Seven canonical sections
 * — Definition, Properties, Samples, Constraints, Mappings,
 * Lineage, Change Log — surface every facet a modeller might shape
 * for a single business concept on one screen.
 *
 * This commit ships the scaffold: header + 7 collapsible sections
 * with placeholder bodies. Subsequent commits replace each
 * placeholder with its live primitive (GlossaryAnchorPicker,
 * PropertyRow, SourceSampleMini, constraint-form,
 * InlineObjectMappingEditor, LineageTree, audit timeline).
 */
export function DomainContextPage({ nodeId }: { nodeId: string }) {
  const t = useTranslations("workbench.types.detail");
  const ontology = useAppStore(selectStateOntology);
  const localeChain = useLocaleChain();

  if (!ontology) {
    return <EmptyShell message={t("noOntology")} />;
  }

  const node = arr(ontology.node_types).find((n) => n.id === nodeId);
  if (!node) {
    return <EmptyShell message={t("nodeNotFound", { id: nodeId })} />;
  }

  const description = localizePresent(node.description, localeChain) ?? "";
  const propertyCount = arr(node.properties).length;
  const constraintCount = arr(node.constraints).length;
  const anchorCount = arr(node.glossary_anchors).length;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <PageHeader
        label={node.label}
        description={description}
        backLabel={t("backToCanvas")}
        validateLabel={t("validateCompleteness")}
      />
      <div className="flex-1 overflow-auto">
        <div className="mx-auto max-w-5xl space-y-3 px-6 py-6">
          <CollapsibleSection
            title={t("sections.definition.title")}
            description={t("sections.definition.subtitle")}
            badge={anchorCount > 0 ? <CountBadge count={anchorCount} /> : undefined}
          >
            <Placeholder hint={t("sections.definition.placeholder")} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.properties.title")}
            description={t("sections.properties.subtitle")}
            badge={<CountBadge count={propertyCount} />}
          >
            <Placeholder hint={t("sections.properties.placeholder")} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.samples.title")}
            description={t("sections.samples.subtitle")}
          >
            <Placeholder hint={t("sections.samples.placeholder")} />
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
            <Placeholder hint={t("sections.constraints.placeholder")} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.mappings.title")}
            description={t("sections.mappings.subtitle")}
            defaultOpen={false}
          >
            <Placeholder hint={t("sections.mappings.placeholder")} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.lineage.title")}
            description={t("sections.lineage.subtitle")}
            defaultOpen={false}
          >
            <Placeholder hint={t("sections.lineage.placeholder")} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.changelog.title")}
            description={t("sections.changelog.subtitle")}
            defaultOpen={false}
          >
            <Placeholder hint={t("sections.changelog.placeholder")} />
          </CollapsibleSection>
        </div>
      </div>
    </div>
  );
}

function PageHeader({
  label,
  description,
  backLabel,
  validateLabel,
}: {
  label: string;
  description: string;
  backLabel: string;
  validateLabel: string;
}) {
  return (
    <header className="flex shrink-0 items-center gap-3 border-b border-zinc-200 bg-white px-6 py-3 dark:border-zinc-800 dark:bg-zinc-950">
      <Link
        href="/design"
        aria-label={backLabel}
        className="rounded p-1 text-muted-foreground hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
      >
        <HugeiconsIcon icon={ArrowLeft01Icon} className="h-4 w-4" size="100%" />
      </Link>
      <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] font-bold uppercase text-emerald-700 dark:bg-emerald-900 dark:text-emerald-400">
        Node
      </span>
      <div className="flex flex-1 flex-col">
        <h1 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          {label}
        </h1>
        {description && (
          <p className="text-[11px] text-muted-foreground">{description}</p>
        )}
      </div>
      <button
        type="button"
        disabled
        className="inline-flex items-center gap-1.5 rounded border border-zinc-200 px-3 py-1.5 text-xs text-muted-foreground opacity-60 dark:border-zinc-800"
        title={validateLabel}
      >
        <HugeiconsIcon icon={Tick02Icon} className="h-3 w-3" size="100%" />
        {validateLabel}
      </button>
    </header>
  );
}

function EmptyShell({ message }: { message: string }) {
  return (
    <div className="flex h-full items-center justify-center px-6 py-12 text-sm text-muted-foreground">
      <div className="max-w-md text-center">{message}</div>
    </div>
  );
}

function CountBadge({ count }: { count: number }) {
  return (
    <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
      {count}
    </span>
  );
}

function Placeholder({ hint }: { hint: string }) {
  return (
    <div className="rounded border border-dashed border-zinc-200 px-3 py-4 text-[11px] text-muted-foreground dark:border-zinc-800">
      {hint}
    </div>
  );
}

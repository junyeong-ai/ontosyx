"use client";

import { useMemo } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";

import type { OntologyIR } from "@/types/api";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  PropertyDef,
} from "@/types/ontology";
import { arr } from "@/lib/ir-collections";

import { Heading } from "@/components/ui/heading";
import { Eyebrow } from "@/components/ui/eyebrow";
// ---------------------------------------------------------------------------
// UsageMap — right pane of the Glossary workbench.
//
// Walks the loaded `OntologyIR` to surface every reverse pointer at
// the selected term: NodeType / EdgeType `glossary_anchors` and
// PropertyDef `bindings[kind=glossary]`. The walk runs in-memory
// against a snapshot the workbench already holds — no extra fetches.
// Empty groups render with a "0" hint instead of being hidden so the
// modeller sees coverage at a glance.
// ---------------------------------------------------------------------------

interface NodeAnchor {
  kind: "node";
  node: NodeTypeDef;
}

interface EdgeAnchor {
  kind: "edge";
  edge: EdgeTypeDef;
}

interface PropertyAnchor {
  kind: "property";
  ownerKind: "node" | "edge";
  ownerId: string;
  ownerLabel: string;
  property: PropertyDef;
}

type Anchor = NodeAnchor | EdgeAnchor | PropertyAnchor;

function collectAnchors(ontology: OntologyIR, termId: string): Anchor[] {
  const out: Anchor[] = [];
  for (const node of arr(ontology.node_types)) {
    if (arr(node.glossary_anchors).includes(termId)) {
      out.push({ kind: "node", node });
    }
    for (const property of arr(node.properties)) {
      const bindings = arr(property.bindings);
      if (
        bindings.some((b) => b.kind === "glossary" && b.id === termId)
      ) {
        out.push({
          kind: "property",
          ownerKind: "node",
          ownerId: node.id,
          ownerLabel: node.label,
          property,
        });
      }
    }
  }
  for (const edge of arr(ontology.edge_types)) {
    if (arr(edge.glossary_anchors).includes(termId)) {
      out.push({ kind: "edge", edge });
    }
    for (const property of arr(edge.properties)) {
      const bindings = arr(property.bindings);
      if (
        bindings.some((b) => b.kind === "glossary" && b.id === termId)
      ) {
        out.push({
          kind: "property",
          ownerKind: "edge",
          ownerId: edge.id,
          ownerLabel: edge.label,
          property,
        });
      }
    }
  }
  return out;
}

interface UsageMapProps {
  ontology: OntologyIR;
  termId: string;
}

export function UsageMap({ ontology, termId }: UsageMapProps) {
  const t = useTranslations("workbench.glossary.usage");
  const anchors = useMemo(
    () => collectAnchors(ontology, termId),
    [ontology, termId],
  );
  const nodes = anchors.filter((a): a is NodeAnchor => a.kind === "node");
  const edges = anchors.filter((a): a is EdgeAnchor => a.kind === "edge");
  const properties = anchors.filter(
    (a): a is PropertyAnchor => a.kind === "property",
  );

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4 text-xs">
      <div>
        <Eyebrow level={2} tone="muted" size="dense" caps="upper">
          {t("heading")}
        </Eyebrow>
        <p className="mt-1 text-2xs text-foreground-muted">
          {t("subtitle", {
            count: anchors.length,
          })}
        </p>
      </div>

      <UsageGroup
        title={t("groups.nodes")}
        count={nodes.length}
        emptyHint={t("groups.empty")}
      >
        {nodes.map(({ node }) => (
          <Link
            key={node.id}
            href={`/design/types/${node.id}`}
            className="flex items-center justify-between rounded border border-divider-soft bg-surface-base px-2.5 py-1.5 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:bg-brand-surface"
          >
            <span className="font-medium text-foreground-strong">
              {node.label}
            </span>
            <span className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-bold uppercase text-brand-foreground-strong">
              {t("badges.node")}
            </span>
          </Link>
        ))}
      </UsageGroup>

      <UsageGroup
        title={t("groups.edges")}
        count={edges.length}
        emptyHint={t("groups.empty")}
      >
        {edges.map(({ edge }) => {
          const src =
            arr(ontology.node_types).find((n) => n.id === edge.source_node_id)
              ?.label ?? "?";
          const tgt =
            arr(ontology.node_types).find((n) => n.id === edge.target_node_id)
              ?.label ?? "?";
          return (
            <div
              key={edge.id}
              className="rounded border border-divider-soft bg-surface-base px-2.5 py-1.5"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium text-foreground-strong">
                  {edge.label}
                </span>
                <span className="rounded bg-concept-surface px-1.5 py-0.5 text-2xs font-bold uppercase text-concept-foreground">
                  {t("badges.edge")}
                </span>
              </div>
              <p className="mt-0.5 font-mono text-2xs text-foreground-muted">
                {src} → {tgt}
              </p>
            </div>
          );
        })}
      </UsageGroup>

      <UsageGroup
        title={t("groups.properties")}
        count={properties.length}
        emptyHint={t("groups.empty")}
      >
        {properties.map((p) => (
          <Link
            key={`${p.ownerId}.${p.property.id}`}
            href={
              p.ownerKind === "node"
                ? `/design/types/${p.ownerId}`
                : "/design"
            }
            className="block rounded border border-divider-soft bg-surface-base px-2.5 py-1.5 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:bg-brand-surface"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="font-mono text-2xs text-foreground-strong">
                {p.ownerLabel}.{p.property.name}
              </span>
            </div>
          </Link>
        ))}
      </UsageGroup>
    </div>
  );
}

function UsageGroup({
  title,
  count,
  emptyHint,
  children,
}: {
  title: string;
  count: number;
  emptyHint: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2">
        <Heading level={3} size={6}>
          {title}
        </Heading>
        <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground-muted">
          {count}
        </span>
      </div>
      {count === 0 ? (
        <p className="text-2xs italic text-foreground-muted">
          {emptyHint}
        </p>
      ) : (
        <div className="flex flex-col gap-1">{children}</div>
      )}
    </section>
  );
}

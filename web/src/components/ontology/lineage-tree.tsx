"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { ArrowLeft, ArrowRight } from "lucide-react";
import type {
  DependencyEdge,
  DependencyKind,
  SchemaEntityRef,
} from "@/lib/api/dependencies";
import { entityRefKey } from "@/lib/api/dependencies";
import { DynamicIcon } from "@/components/ui/dynamic-icon";

export type LineageDirection = "inbound" | "outbound";

export interface LineageTreeProps {
  /** Edges to render. Direction is implicit in the bucket they came
   *  from — the consumer feeds either `dependents_of` (inbound) or
   *  `references_of` (outbound). */
  edges: readonly DependencyEdge[];
  /** Inbound = "what depends on me"; outbound = "what I reference". */
  direction: LineageDirection;
  /** Resolve a ref to a human label. When omitted, the picker falls
   *  back to the ref's id. */
  labelOf?: (ref: SchemaEntityRef) => string | null;
  /** Click-through navigation. When omitted, endpoints render as
   *  plain text rows. */
  onSelect?: (ref: SchemaEntityRef) => void;
  /** Override the empty-state message. Defaults to direction-aware
   *  i18n strings. */
  emptyLabel?: string;
}

interface KindGroup {
  kind: DependencyKind;
  edges: readonly DependencyEdge[];
}

/**
 * Render a flat dependency tree grouped by [`DependencyKind`].
 *
 * Direction-agnostic on the data shape — `inbound` or `outbound`
 * decides arrow rendering and the empty-state copy, but the row
 * structure is identical so the same component serves both
 * Inspector tabs (Lineage / Dependents) and the Domain Context
 * Lineage section.
 */
export function LineageTree({
  edges,
  direction,
  labelOf,
  onSelect,
  emptyLabel,
}: LineageTreeProps) {
  const t = useTranslations("ontology.lineageTree");

  const groups = useMemo<KindGroup[]>(() => {
    const byKind = new Map<DependencyKind, DependencyEdge[]>();
    for (const edge of edges) {
      const bucket = byKind.get(edge.kind);
      if (bucket) bucket.push(edge);
      else byKind.set(edge.kind, [edge]);
    }
    return [...byKind.entries()]
      .map(([kind, edges]) => ({ kind, edges }))
      .sort((a, b) => a.kind.localeCompare(b.kind));
  }, [edges]);

  if (groups.length === 0) {
    return (
      <p className="px-1 py-2 text-2xs italic text-foreground-muted">
        {emptyLabel ??
          (direction === "inbound" ? t("emptyInbound") : t("emptyOutbound"))}
      </p>
    );
  }

  return (
    <ul className="space-y-2">
      {groups.map((group) => (
        <li key={group.kind}>
          <KindGroupBlock
            group={group}
            direction={direction}
            labelOf={labelOf}
            onSelect={onSelect}
            kindLabel={t(`kinds.${group.kind}`)}
          />
        </li>
      ))}
    </ul>
  );
}

function KindGroupBlock({
  group,
  direction,
  labelOf,
  onSelect,
  kindLabel,
}: {
  group: KindGroup;
  direction: LineageDirection;
  labelOf: ((ref: SchemaEntityRef) => string | null) | undefined;
  onSelect: ((ref: SchemaEntityRef) => void) | undefined;
  kindLabel: string;
}) {
  return (
    <div className="rounded border border-divider-soft">
      <header className="flex items-center justify-between gap-2 bg-surface-raised px-2 py-1">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {kindLabel}
        </span>
        <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground-muted">
          {group.edges.length}
        </span>
      </header>
      <ul className="divide-y divide-divider-soft/40">
        {group.edges.map((edge) => (
          <EdgeRow
            key={`${entityRefKey(edge.endpoint)}::${edge.label}`}
            edge={edge}
            direction={direction}
            labelOf={labelOf}
            onSelect={onSelect}
          />
        ))}
      </ul>
    </div>
  );
}

function EdgeRow({
  edge,
  direction,
  labelOf,
  onSelect,
}: {
  edge: DependencyEdge;
  direction: LineageDirection;
  labelOf: ((ref: SchemaEntityRef) => string | null) | undefined;
  onSelect: ((ref: SchemaEntityRef) => void) | undefined;
}) {
  const label = labelOf?.(edge.endpoint) ?? defaultLabel(edge.endpoint);
  const arrow = direction === "inbound" ? ArrowLeft : ArrowRight;
  const interactive = !!onSelect;
  const Cell = interactive ? "button" : "div";
  return (
    <li>
      <Cell
        type={interactive ? "button" : undefined}
        onClick={interactive ? () => onSelect!(edge.endpoint) : undefined}
        className={
          "flex w-full items-center gap-2 px-2 py-1 text-start text-2xs " +
          (interactive
            ? "hover:bg-concept-surface"
            : "")
        }
      >
        <DynamicIcon as={arrow} className="h-2.5 w-2.5 shrink-0 text-foreground-muted" />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium text-foreground-strong">
            {label}
          </span>
          <span className="truncate text-2xs text-foreground-muted">
            {edge.label}
          </span>
        </span>
        <span className="shrink-0 rounded bg-surface-inset px-1 text-2xs font-mono text-foreground-muted">
          {edge.endpoint.kind}
        </span>
      </Cell>
    </li>
  );
}

function defaultLabel(ref: SchemaEntityRef): string {
  switch (ref.kind) {
    case "property":
      return `${ref.owner}.${ref.id}`;
    case "coded_value":
      return ref.code_system ? `${ref.code_system}/${ref.id}` : ref.id;
    default:
      return ref.id;
  }
}

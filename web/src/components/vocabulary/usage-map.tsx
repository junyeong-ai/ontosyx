"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";

import type { OntologyIR } from "@/types/api";
import type { PropertyDef } from "@/types/ontology";
import { arr } from "@/lib/ir-collections";

// ---------------------------------------------------------------------------
// VocabularyUsageMap — reverse-pointer panel for vocabulary entities.
//
// One canonical entry shape (`UsageEntry`) covers every reference path
// from a vocabulary entity (CodeSystem / ValueSet / ConceptMap /
// NotationPattern) back to the consuming surface — typically a
// PropertyDef on a NodeType or EdgeType, or another vocabulary entity
// that composes / references the source. The walkers below produce
// these entries; the page renders them grouped + clickable.
// ---------------------------------------------------------------------------

export interface UsageEntry {
  /** Routing target for the click — usually deep links to the
   *  owner's design canvas page. */
  href: string;
  /** Owner the consumer lives under — `"Customer"`, `"PURCHASED"`,
   *  `"vs-order-status"`. */
  ownerLabel: string;
  /** Short description of the consuming role — `"status property"`,
   *  `"source system"`, `"composes"`. */
  detail: string;
  /** Stable react key — caller-supplied. */
  key: string;
  /** Pill-style classification rendered to the right of the owner
   *  label. */
  badge: "node" | "edge" | "value_set" | "concept_map";
}

interface PropertyAnchor {
  ownerKind: "node" | "edge";
  ownerId: string;
  ownerLabel: string;
  property: PropertyDef;
}

function walkProperties(
  ontology: OntologyIR,
  match: (
    binding: NonNullable<PropertyDef["bindings"]>[number],
  ) => boolean,
): PropertyAnchor[] {
  const out: PropertyAnchor[] = [];
  for (const node of arr(ontology.node_types)) {
    for (const property of arr(node.properties)) {
      if (arr(property.bindings).some(match)) {
        out.push({
          ownerKind: "node",
          ownerId: node.id,
          ownerLabel: node.label,
          property,
        });
      }
    }
  }
  for (const edge of arr(ontology.edge_types)) {
    for (const property of arr(edge.properties)) {
      if (arr(property.bindings).some(match)) {
        out.push({
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

function propertyEntry(anchor: PropertyAnchor): UsageEntry {
  return {
    key: `prop:${anchor.ownerKind}:${anchor.ownerId}:${anchor.property.id}`,
    href:
      anchor.ownerKind === "node"
        ? `/design/types/${anchor.ownerId}?prop=${encodeURIComponent(anchor.property.id)}`
        : `/design?edge=${encodeURIComponent(anchor.ownerId)}&prop=${encodeURIComponent(anchor.property.id)}`,
    ownerLabel: anchor.ownerLabel,
    detail: anchor.property.name,
    badge: anchor.ownerKind === "node" ? "node" : "edge",
  };
}

export function collectCodeSystemUsages(
  ontology: OntologyIR,
  codeSystemId: string,
): UsageEntry[] {
  const out: UsageEntry[] = [];

  const directProperties = walkProperties(
    ontology,
    (b) => b.kind === "code_system" && b.id === codeSystemId,
  );
  for (const anchor of directProperties) out.push(propertyEntry(anchor));

  for (const valueSet of arr(ontology.value_sets)) {
    const composed = arr(valueSet.composition).some(
      (c) => c.system_id === codeSystemId,
    );
    if (composed) {
      out.push({
        key: `vs:${valueSet.id}`,
        href: `/vocabulary?tab=value_sets&id=${encodeURIComponent(valueSet.id)}`,
        ownerLabel: valueSet.name ?? valueSet.id,
        detail: "composes",
        badge: "value_set",
      });
    }
  }

  for (const conceptMap of arr(ontology.concept_maps)) {
    if (
      conceptMap.source_system_id === codeSystemId ||
      conceptMap.target_system_id === codeSystemId
    ) {
      const role =
        conceptMap.source_system_id === codeSystemId ? "source" : "target";
      out.push({
        key: `cm:${conceptMap.id}:${role}`,
        href: `/vocabulary?tab=concept_maps&id=${encodeURIComponent(conceptMap.id)}`,
        ownerLabel: conceptMap.name ?? conceptMap.id,
        detail: role,
        badge: "concept_map",
      });
    }
  }

  return out;
}

export function collectValueSetUsages(
  ontology: OntologyIR,
  valueSetId: string,
): UsageEntry[] {
  return walkProperties(
    ontology,
    (b) => b.kind === "value_set" && b.id === valueSetId,
  ).map(propertyEntry);
}

export function collectConceptMapUsages(
  ontology: OntologyIR,
  conceptMapId: string,
): UsageEntry[] {
  return walkProperties(ontology, (b) => {
    const id = (b as { concept_map_id?: string }).concept_map_id;
    return id === conceptMapId;
  }).map(propertyEntry);
}

export function collectNotationPatternUsages(
  ontology: OntologyIR,
  notationPatternId: string,
): UsageEntry[] {
  return walkProperties(
    ontology,
    (b) => b.kind === "notation_pattern" && b.id === notationPatternId,
  ).map(propertyEntry);
}

interface VocabularyUsageMapProps {
  entries: UsageEntry[];
}

export function VocabularyUsageMap({ entries }: VocabularyUsageMapProps) {
  const t = useTranslations("settings.vocabulary.usage");

  if (entries.length === 0) {
    return (
      <p className="text-2xs text-foreground-muted">{t("none")}</p>
    );
  }

  return (
    <ul className="flex flex-col gap-1">
      {entries.map((entry) => (
        <li key={entry.key}>
          <Link
            href={entry.href}
            className="flex items-center justify-between gap-2 rounded border border-divider-soft bg-surface-base px-2 py-1.5 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:bg-brand-surface"
          >
            <span className="flex min-w-0 items-center gap-1.5">
              <span className="truncate text-2xs font-medium text-foreground">
                {entry.ownerLabel}
              </span>
              <span className="truncate font-mono text-2xs text-foreground-muted">
                {entry.detail}
              </span>
            </span>
            <UsageBadge variant={entry.badge} />
          </Link>
        </li>
      ))}
    </ul>
  );
}

function UsageBadge({ variant }: { variant: UsageEntry["badge"] }) {
  const t = useTranslations("settings.vocabulary.usage.badges");
  const styles: Record<UsageEntry["badge"], string> = {
    node: "bg-brand-surface-strong text-brand-foreground-strong",
    edge: "bg-concept-surface text-concept-foreground",
    value_set: "bg-info-surface text-info-foreground",
    concept_map: "bg-warning-surface text-warning-foreground",
  };
  return (
    <span
      className={`shrink-0 rounded px-1.5 py-0.5 text-2xs font-bold uppercase ${styles[variant]}`}
    >
      {t(variant)}
    </span>
  );
}

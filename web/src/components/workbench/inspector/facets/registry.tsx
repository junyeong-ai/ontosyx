"use client";

// Inspector facet registry — single source of truth for the tabbed
// facets that render on the right-hand inspector and on the entity
// detail page.
//
// Adding a new facet (e.g. a "Permissions" tab on a future RBAC
// surface) is a one-entry push into `INSPECTOR_FACETS` — the inspector
// shell, the help dialog, the URL deep-link, and the tab-memory store
// all read through the same array, so a new entry is uniformly
// discoverable. The contract is thin on purpose: each facet owns its
// own padding / Section chrome via `render`, which keeps the host
// agnostic about layout decisions per pane.
//
// The registry is intentionally NOT mutable at runtime. A plugin
// system that needs runtime registration would expose `registerFacet`
// + `unregisterFacet` on top of this constant; today every facet
// ships with the workbench itself and the `as const` keeps the
// `InspectorFacetId` literal type narrow for compile-time exhaustive
// checks.

import { useTranslations } from "next-intl";
import type { ReactNode } from "react";

import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import type { SchemaEntityRef } from "@/lib/api/dependencies";
import { arr } from "@/lib/ir-collections";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyIR,
  QualityGap,
} from "@/types/api";

import { ChangeLogFacet } from "./change-log-facet";
import { ConstraintsFacet } from "./constraints-facet";
import { DefinitionFacet } from "./definition-facet";
import { LineageFacet } from "./lineage-facet";
import { MappingsFacet } from "./mappings-facet";
import { PropertiesFacet } from "./properties-facet";
import { QualityFacet } from "./quality-facet";
import { RulesFacet } from "./rules-facet";
import { SamplesFacet } from "./samples-facet";
import { Section } from "../shared";

export type InspectorEntityKind = "node" | "edge";

/**
 * The seven built-in facet ids. Plugins can register additional ids
 * — `InspectorFacetId` is a `string` at the type level so the
 * registry stays open. Convenience union below documents what's
 * shipped by default; consumers that need exhaustiveness over the
 * defaults import that, callers that handle arbitrary registered
 * facets stay on the wider `string`.
 */
export type DefaultInspectorFacetId =
  | "definition"
  | "mappings"
  | "sample"
  | "lineage"
  | "rules"
  | "quality"
  | "changelog";
export type InspectorFacetId = string;

/**
 * Read-only context every facet receives. Discriminate on `kind`
 * for narrow access — the `node` / `edge` slots are populated
 * exactly when the matching kind is active.
 */
export interface InspectorFacetContext {
  ontology: OntologyIR;
  kind: InspectorEntityKind;
  entityRef: SchemaEntityRef;
  /** Generic entity handle — useful for facets that work on both kinds. */
  entity: NodeTypeDef | EdgeTypeDef;
  /** Populated when `kind === "node"`. */
  node: NodeTypeDef | null;
  /** Populated when `kind === "edge"`. */
  edge: EdgeTypeDef | null;
  /** Quality gaps assigned to this entity. */
  gaps: QualityGap[];
  /** Lineage badge counts (incoming + outgoing dependents). */
  inboundCount: number;
  outboundCount: number;
}

export interface InspectorFacet {
  id: InspectorFacetId;
  /** i18n key resolved through the `inspector.tabs` namespace. */
  labelKey: string;
  /** Returns true if this facet is meaningful for the current entity. */
  accept: (ctx: InspectorFacetContext) => boolean;
  /** Optional badge shown on the tab strip. Returning `undefined` hides it. */
  badge?: (ctx: InspectorFacetContext) => number | undefined;
  /** Render the facet body. The host wraps this in scroll chrome only. */
  render: (ctx: InspectorFacetContext) => ReactNode;
}

// ---------------------------------------------------------------------------
// Compound "Definition" pane — Glossary + Properties (every kind)
// followed by Constraints + Mappings (node-only). Lives next to the
// registry so the cohesion is obvious; consumers never reach in.
// ---------------------------------------------------------------------------

function DefinitionPane({ ctx }: { ctx: InspectorFacetContext }) {
  const t = useTranslations("inspector.tabs");
  const tEntity = useTranslations("inspector.entity");
  const propertyCount = arr(ctx.entity.properties).length;
  return (
    <>
      <Section title={t("definitionSubsection.glossary")}>
        <div className="px-3 py-2">
          <DefinitionFacet
            ontology={ctx.ontology}
            entity={ctx.entity}
            kind={ctx.kind}
          />
        </div>
      </Section>
      <Section title={`Properties (${propertyCount})`}>
        <div className="px-3 py-2">
          <PropertiesFacet
            ontology={ctx.ontology}
            entity={ctx.entity}
            kind={ctx.kind}
          />
        </div>
      </Section>
      {ctx.node && (
        <Section title={`Constraints (${arr(ctx.node.constraints).length})`}>
          <div className="px-3 py-2">
            <ConstraintsFacet node={ctx.node} />
          </div>
        </Section>
      )}
      <p className="mt-3 px-3 pb-2 text-2xs text-foreground-muted">
        {tEntity.rich("editTip", {
          kbd: () => <KeyboardShortcut keys="mod+k" />,
        })}
      </p>
    </>
  );
}

// ---------------------------------------------------------------------------
// Registry — runtime-mutable, ordered, dedup-by-id
// ---------------------------------------------------------------------------
//
// The five default facets ship pre-registered below; callers (the
// inspector chrome, tests, future extension code) read through the
// public API rather than the array. A plugin module that wants to
// add a `Permissions` facet does:
//
//     import { registerInspectorFacet } from "@/components/workbench/inspector/facets/registry";
//     registerInspectorFacet({ id: "permissions", ... });
//
// — there is no need to fork this file. Re-registering an existing
// id replaces the prior entry (idempotent registration), so HMR and
// double-mount don't accumulate ghosts.

const DEFAULT_FACETS: readonly InspectorFacet[] = [
  {
    id: "definition",
    labelKey: "definition",
    accept: () => true,
    render: (ctx) => <DefinitionPane ctx={ctx} />,
  },
  {
    id: "mappings",
    labelKey: "mappings",
    // Object mappings are a node-only concept — edges bind through
    // their endpoint nodes, so the facet hides on edge entities.
    accept: (ctx) => ctx.kind === "node",
    badge: (ctx) =>
      ctx.node
        ? arr(ctx.ontology.object_mappings).filter(
            (m) => m.node_type_id === ctx.entity.id,
          ).length || undefined
        : undefined,
    render: (ctx) =>
      ctx.node ? (
        <div className="px-3 py-3">
          <MappingsFacet node={ctx.node} ontology={ctx.ontology} />
        </div>
      ) : null,
  },
  {
    id: "sample",
    labelKey: "sample",
    // The Samples facet renders source-row previews and value
    // distributions — only meaningful for nodes that have a backing
    // table in their lineage. Edges and constraint-only nodes hide it.
    accept: (ctx) => Boolean(ctx.node?.source_lineage?.table),
    render: (ctx) =>
      ctx.node ? (
        <div className="px-3 py-3">
          <SamplesFacet node={ctx.node} />
        </div>
      ) : null,
  },
  {
    id: "lineage",
    labelKey: "lineage",
    accept: () => true,
    badge: (ctx) =>
      ctx.inboundCount + ctx.outboundCount || undefined,
    render: (ctx) => (
      <div className="px-3 py-3">
        <LineageFacet ontology={ctx.ontology} entityRef={ctx.entityRef} />
      </div>
    ),
  },
  {
    id: "rules",
    labelKey: "rules",
    accept: () => true,
    badge: (ctx) => {
      const count = arr(ctx.ontology.rules).filter((rule) => {
        const target = rule.kind;
        if (!target) return false;
        if (ctx.kind === "node") {
          return (
            ("target_node_type_id" in target &&
              target.target_node_type_id === ctx.entity.id) ||
            ("state_property_id" in target &&
              target.target_node_type_id === ctx.entity.id)
          );
        }
        return (
          "target_edge_type_id" in target &&
          target.target_edge_type_id === ctx.entity.id
        );
      }).length;
      return count || undefined;
    },
    render: (ctx) => (
      <div className="px-3 py-3">
        <RulesFacet
          ontology={ctx.ontology}
          entity={ctx.entity}
          kind={ctx.kind}
        />
      </div>
    ),
  },
  {
    id: "quality",
    labelKey: "quality",
    accept: () => true,
    badge: (ctx) => ctx.gaps.length || undefined,
    render: (ctx) => (
      <div className="px-3 py-3">
        <QualityFacet gaps={ctx.gaps} />
      </div>
    ),
  },
  {
    id: "changelog",
    labelKey: "changelog",
    accept: () => true,
    render: (ctx) => (
      <div className="px-3 py-3">
        <ChangeLogFacet
          ontology={ctx.ontology}
          entity={ctx.entity}
          kind={ctx.kind}
        />
      </div>
    ),
  },
];

// Module-singleton state — backed by the generic `PluginRegistry`
// from `lib/plugins/registry.ts`. Default facets seed the registry
// at load time; downstream modules add more via `registerInspectorFacet`.
import { PluginRegistry } from "@/lib/plugins/registry";

const facetRegistry = new PluginRegistry<InspectorFacet>();
for (const facet of DEFAULT_FACETS) facetRegistry.register(facet);

export interface RegisterFacetOptions {
  /** Insert before the named facet id; falls back to append if absent. */
  before?: string;
  /** Insert after the named facet id; falls back to append if absent. */
  after?: string;
}

/**
 * Register or replace a facet. Re-registering an existing id
 * replaces the payload while preserving its position; a fresh id
 * appends unless `before` / `after` is supplied.
 */
export function registerInspectorFacet(
  facet: InspectorFacet,
  options: RegisterFacetOptions = {},
): void {
  facetRegistry.register(facet, options);
}

/** Remove a previously registered facet. Idempotent — unknown ids no-op. */
export function unregisterInspectorFacet(id: string): void {
  facetRegistry.unregister(id);
}

/**
 * Snapshot of all currently-registered facets in declared order.
 * Useful for debugging surfaces and the help dialog inventory.
 */
export function listInspectorFacets(): InspectorFacet[] {
  return facetRegistry.list();
}

/** Iterable alias — many call sites read this array. */
export const INSPECTOR_FACETS = {
  get length() {
    return facetRegistry.list().length;
  },
  [Symbol.iterator]() {
    return facetRegistry.list()[Symbol.iterator]();
  },
} as Iterable<InspectorFacet> & { length: number };

/**
 * Resolve a facet by id. Returns `undefined` if the id is not
 * registered — callers fall back to the first visible facet.
 */
export function inspectorFacetById(
  id: string,
): InspectorFacet | undefined {
  return facetRegistry.get(id);
}

/**
 * The visible facets for a given context, in declared order.
 * Used by the tab strip and the body switch.
 */
export function visibleInspectorFacets(
  ctx: InspectorFacetContext,
): InspectorFacet[] {
  return listInspectorFacets().filter((f) => f.accept(ctx));
}

// Test-only escape hatch: reset the registry to the default-shipping
// set. Keeps registration tests hermetic without exposing mutation
// surface to production code.
export function _resetInspectorFacetRegistryForTests(): void {
  facetRegistry.clearForTests();
  for (const f of DEFAULT_FACETS) facetRegistry.register(f);
}

"use client";

import { useCallback } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowLeft01Icon,
  Cancel01Icon,
  PlusSignIcon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";
import { useState } from "react";
import { toast } from "sonner";

import { useAppStore } from "@/lib/store";
import { selectStateOntology } from "@/lib/store/selectors";
import { arr } from "@/lib/ir-collections";
import { CollapsibleSection } from "@/components/ui/collapsible-section";
import { GlossaryAnchorPicker } from "@/components/ontology/glossary-anchor-picker";
import { NodeConstraintBuilder } from "@/components/ontology/node-constraint-builder";
import { InlineObjectMappingEditor } from "@/components/ontology/inline-object-mapping-editor";
import { LineageTree } from "@/components/ontology/lineage-tree";
import { SourceSampleMini } from "@/components/workbench/inspector/source-sample-mini";
import {
  AddPropertyForm,
  PropertyRow,
} from "@/components/workbench/inspector/property-editor";
import { InlineEdit } from "@/components/workbench/inspector/inline-edit";
import { defaultText } from "@/lib/locale/localize";
import type {
  ConstraintDef,
  NodeTypeDef,
  OntologyCommand,
  OntologyIR,
  PropertyPatch,
} from "@/types/api";
import type {
  GlossaryTermDef,
  ObjectMappingDef,
} from "@/lib/api/edit-ops";
import { useEntityDependencies } from "@/hooks/api/use-entity-dependencies";
import type { SchemaEntityRef } from "@/lib/api/dependencies";
import { Tooltip } from "@/components/ui/tooltip";

/**
 * Domain Context page for one NodeType. Seven canonical sections
 * — Definition, Properties, Samples, Constraints, Mappings,
 * Lineage, Change Log — surface every facet a modeller might shape
 * for a single business concept on one screen.
 *
 * This commit wires Definition + Properties:
 * - Definition: inline label + description editing, GlossaryAnchorPicker
 *   bound to `set_node_glossary_anchors` command (atomic list replace).
 * - Properties: reuses the inspector's PropertyRow + AddPropertyForm,
 *   so every property edit flows through the same applyCommand →
 *   commandStack → save pipeline as the canvas inspector.
 *
 * Subsequent commits replace each remaining placeholder
 * (samples, constraints, mappings, lineage, changelog).
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

// ---------------------------------------------------------------------------
// NodeView — owns the per-node hooks once we know the node resolved
// ---------------------------------------------------------------------------

function NodeView({
  ontology,
  node,
}: {
  ontology: OntologyIR;
  node: NodeTypeDef;
}) {
  const t = useTranslations("workbench.types.detail");
  const applyCommand = useAppStore((s) => s.applyCommand);

  const propertyCount = arr(node.properties).length;
  const constraintCount = arr(node.constraints).length;
  const anchors = arr(node.glossary_anchors);
  const glossary: readonly GlossaryTermDef[] = arr(ontology.glossary);

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

  const handleUpdateDescription = useCallback(
    (desc: string) => {
      applyCommand({
        op: "update_node_description",
        node_id: node.id,
        description: desc ? { default: desc } : undefined,
      });
    },
    [applyCommand, node.id],
  );

  const handleAnchorsChange = useCallback(
    (next: string[]) => {
      applyCommand({
        op: "set_node_glossary_anchors",
        node_id: node.id,
        anchors: next,
      });
    },
    [applyCommand, node.id],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <PageHeader
        label={node.label}
        backLabel={t("backToCanvas")}
        validateLabel={t("validateCompleteness")}
        onRename={handleRename}
      />
      <div className="flex-1 overflow-auto">
        <div className="mx-auto max-w-5xl space-y-3 px-6 py-6">
          <CollapsibleSection
            title={t("sections.definition.title")}
            description={t("sections.definition.subtitle")}
            badge={anchors.length > 0 ? <CountBadge count={anchors.length} /> : undefined}
          >
            <DefinitionSection
              node={node}
              glossary={glossary}
              onUpdateDescription={handleUpdateDescription}
              onAnchorsChange={handleAnchorsChange}
            />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.properties.title")}
            description={t("sections.properties.subtitle")}
            badge={<CountBadge count={propertyCount} />}
          >
            <PropertiesSection node={node} ontology={ontology} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.samples.title")}
            description={t("sections.samples.subtitle")}
          >
            <SamplesSection node={node} />
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
            <ConstraintsSection node={node} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.mappings.title")}
            description={t("sections.mappings.subtitle")}
            defaultOpen={false}
          >
            <MappingsSection node={node} ontology={ontology} />
          </CollapsibleSection>

          <CollapsibleSection
            title={t("sections.lineage.title")}
            description={t("sections.lineage.subtitle")}
            defaultOpen={false}
          >
            <LineageSection node={node} ontology={ontology} />
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

// ---------------------------------------------------------------------------
// Definition section
// ---------------------------------------------------------------------------

function DefinitionSection({
  node,
  glossary,
  onUpdateDescription,
  onAnchorsChange,
}: {
  node: NodeTypeDef;
  glossary: readonly GlossaryTermDef[];
  onUpdateDescription: (desc: string) => void;
  onAnchorsChange: (next: string[]) => void;
}) {
  const t = useTranslations("workbench.types.detail.definition");
  const description = defaultText(node.description);

  return (
    <div className="space-y-4">
      <FieldGroup label={t("descriptionLabel")}>
        <InlineEdit
          value={description}
          placeholder={t("descriptionPlaceholder")}
          onSave={onUpdateDescription}
          className="text-zinc-700 dark:text-zinc-200"
        />
      </FieldGroup>

      <FieldGroup
        label={t("anchorsLabel")}
        hint={t("anchorsHint")}
      >
        <GlossaryAnchorPicker
          value={arr(node.glossary_anchors)}
          glossary={glossary}
          onChange={onAnchorsChange}
        />
      </FieldGroup>

      {node.source_lineage?.table && (
        <FieldGroup label={t("sourceLineageLabel")}>
          <p className="font-mono text-[11px] text-muted-foreground">
            {node.source_lineage.table}
          </p>
        </FieldGroup>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Properties section
// ---------------------------------------------------------------------------

function PropertiesSection({
  node,
  ontology,
}: {
  node: NodeTypeDef;
  ontology: OntologyIR;
}) {
  const t = useTranslations("workbench.types.detail.properties");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const [adding, setAdding] = useState(false);

  const handleDelete = useCallback(
    (propId: string, propName: string) => {
      applyCommand({
        op: "delete_property",
        owner_id: node.id,
        property_id: propId,
      });
      toast.success(t("deletedToast", { name: propName }));
    },
    [applyCommand, node.id, t],
  );

  const handleUpdate = useCallback(
    (propId: string, patch: PropertyPatch) => {
      applyCommand({
        op: "update_property",
        owner_id: node.id,
        property_id: propId,
        patch,
      });
    },
    [applyCommand, node.id],
  );

  const properties = arr(node.properties);

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-end">
        {!adding && (
          <Tooltip content={t("addAction")}>
            <button
              type="button"
              onClick={() => setAdding(true)}
              className="inline-flex items-center gap-1 rounded border border-dashed border-zinc-300 px-2 py-1 text-[11px] text-muted-foreground hover:border-emerald-300 hover:text-emerald-600 dark:border-zinc-700 dark:hover:border-emerald-700 dark:hover:text-emerald-400"
            >
              <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
              {t("addAction")}
            </button>
          </Tooltip>
        )}
      </div>
      {adding && (
        <AddPropertyForm ownerId={node.id} onClose={() => setAdding(false)} />
      )}
      {properties.length === 0 && !adding ? (
        <p className="text-[11px] italic text-muted-foreground">
          {t("emptyState")}
        </p>
      ) : (
        <ul className="divide-y divide-zinc-100 rounded border border-zinc-100 dark:divide-zinc-800/60 dark:border-zinc-800/60">
          {properties.map((prop) => (
            <li key={prop.id}>
              <PropertyRow
                prop={prop}
                onDelete={() => handleDelete(prop.id, prop.name)}
                onUpdate={(patch) => handleUpdate(prop.id, patch)}
                binding={
                  ontology.id
                    ? {
                        ontologyId: ontology.id,
                        expectedVersion: ontology.version,
                        ownerKind: "node",
                        ownerTypeId: node.id,
                      }
                    : undefined
                }
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Samples section
// ---------------------------------------------------------------------------

function SamplesSection({ node }: { node: NodeTypeDef }) {
  const t = useTranslations("workbench.types.detail.samples");
  const table = node.source_lineage?.table;
  if (!table) {
    return (
      <p className="text-[11px] italic text-muted-foreground">
        {t("noSourceLineage")}
      </p>
    );
  }
  return <SourceSampleMini tableName={table} />;
}

// ---------------------------------------------------------------------------
// Constraints section
// ---------------------------------------------------------------------------

function ConstraintsSection({ node }: { node: NodeTypeDef }) {
  const t = useTranslations("workbench.types.detail.constraints");
  const applyCommand = useAppStore((s) => s.applyCommand);

  const handleAdd = useCallback(
    (constraint: ConstraintDef) => {
      applyCommand({
        op: "add_constraint",
        node_id: node.id,
        constraint,
      });
      toast.success(t("addedToast"));
    },
    [applyCommand, node.id, t],
  );

  const handleRemove = useCallback(
    (constraintId: string) => {
      applyCommand({
        op: "remove_constraint",
        node_id: node.id,
        constraint_id: constraintId,
      });
      toast.success(t("removedToast"));
    },
    [applyCommand, node.id, t],
  );

  return (
    <NodeConstraintBuilder
      node={node}
      onAdd={handleAdd}
      onRemove={handleRemove}
    />
  );
}

// ---------------------------------------------------------------------------
// Mappings section
// ---------------------------------------------------------------------------

function MappingsSection({
  node,
  ontology,
}: {
  node: NodeTypeDef;
  ontology: OntologyIR;
}) {
  const t = useTranslations("workbench.types.detail.mappings");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const project = useAppStore((s) => s.activeProject);

  const mappings = arr(ontology.object_mappings).filter(
    (m) => m.node_type_id === node.id,
  );

  // Single-mapping is the common case the inline editor targets.
  // The multi-mapping admin path stays on /settings/mappings.
  const primary = mappings[0];
  const additional = mappings.length > 1 ? mappings.length - 1 : 0;

  const sourceColumns: readonly string[] | undefined = (() => {
    if (!primary?.relation || !project?.source_profile) return undefined;
    const profile = project.source_profile.table_profiles?.find(
      (tp) => tp.table_name === primary.relation,
    );
    return profile?.column_stats.map((c) => c.column_name);
  })();

  const handleCreate = () => {
    if (!project?.source_id) {
      toast.error(t("createBlockedNoSource"));
      return;
    }
    const mapping: ObjectMappingDef = {
      id: `om-${crypto.randomUUID()}`,
      node_type_id: node.id,
      source_id: project.source_id,
      relation: "",
      relation_kind: "table",
      property_mappings: [],
    };
    applyCommand({ op: "create_object_mapping", mapping });
    toast.success(t("createdToast"));
  };

  const handleUpdate = (mapping: ObjectMappingDef) => {
    applyCommand({ op: "update_object_mapping", id: mapping.id, mapping });
  };

  const handleDelete = (id: string) => {
    applyCommand({ op: "delete_object_mapping", id });
    toast.success(t("deletedToast"));
  };

  if (!primary) {
    return (
      <div className="space-y-2">
        <p className="text-[11px] italic text-muted-foreground">
          {t("emptyState")}
        </p>
        <button
          type="button"
          onClick={handleCreate}
          disabled={!project?.source_id}
          className="inline-flex items-center gap-1 rounded border border-dashed border-zinc-300 px-2 py-1 text-[11px] text-muted-foreground hover:border-violet-300 hover:text-violet-600 disabled:opacity-50 dark:border-zinc-700 dark:hover:border-violet-700 dark:hover:text-violet-400"
        >
          {t("createAction")}
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[10px] font-mono text-muted-foreground">
          {t("primaryLabel", { id: primary.id })}
        </span>
        <Tooltip content={t("deleteTooltip")}>
          <button
            type="button"
            onClick={() => handleDelete(primary.id)}
            aria-label={t("deleteTooltip")}
            className="rounded p-0.5 text-zinc-300 hover:bg-rose-50 hover:text-rose-500 dark:hover:bg-rose-950"
          >
            <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
      </div>
      <InlineObjectMappingEditor
        value={primary}
        properties={arr(node.properties)}
        availableColumns={sourceColumns}
        onChange={handleUpdate}
      />
      {additional > 0 && (
        <p className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] text-amber-800 dark:border-amber-900/40 dark:bg-amber-950/30 dark:text-amber-300">
          {t("multiMappingHint", { count: additional })}
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Lineage section
// ---------------------------------------------------------------------------

function LineageSection({
  node,
  ontology,
}: {
  node: NodeTypeDef;
  ontology: OntologyIR;
}) {
  const t = useTranslations("workbench.types.detail.lineage");
  const ref: SchemaEntityRef = { kind: "node_type", id: node.id };
  const { inbound, outbound, isLoading } = useEntityDependencies(
    ontology.id,
    ref,
  );

  const labelOf = useCallback(
    (target: SchemaEntityRef): string | null => {
      switch (target.kind) {
        case "node_type":
          return arr(ontology.node_types).find((n) => n.id === target.id)?.label ?? null;
        case "edge_type":
          return arr(ontology.edge_types).find((e) => e.id === target.id)?.label ?? null;
        default:
          return null;
      }
    },
    [ontology],
  );

  if (isLoading) {
    return (
      <p className="text-[11px] italic text-muted-foreground">
        {t("loading")}
      </p>
    );
  }

  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
      <div className="space-y-1">
        <h3 className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {t("outboundHeader")}
        </h3>
        <LineageTree
          edges={outbound}
          direction="outbound"
          labelOf={labelOf}
        />
      </div>
      <div className="space-y-1">
        <h3 className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {t("inboundHeader")}
        </h3>
        <LineageTree edges={inbound} direction="inbound" labelOf={labelOf} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page chrome
// ---------------------------------------------------------------------------

function PageHeader({
  label,
  backLabel,
  validateLabel,
  onRename,
}: {
  label: string;
  backLabel: string;
  validateLabel: string;
  onRename: (next: string) => void;
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
        <InlineEdit
          value={label}
          onSave={onRename}
          className="text-sm font-semibold text-zinc-900 dark:text-zinc-100"
        />
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

function FieldGroup({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <div className="flex items-baseline gap-2">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </span>
        {hint && (
          <span className="text-[10px] text-muted-foreground">{hint}</span>
        )}
      </div>
      {children}
    </div>
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

// `applyCommand` is read inline at every call site via `useAppStore`.
// This re-export keeps a stable handle for sub-components that may
// want to compose multiple commands in a single user action without
// re-querying the store on every render.
export type { OntologyCommand };

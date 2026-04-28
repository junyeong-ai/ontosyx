"use client";

// /settings/mappings — canonical CRUD page for the physical-to-
// logical mapping layer (ObjectMapping + LinkMapping).
//
// The mapping shapes carry rich physical-layer metadata
// (ColumnRef, SourceRelationRef, branching LinkMappingKind
// variants); the editor uses the JSON dual-mode pattern (cf. dbt's
// model YAML) so operators always have a power-user surface that
// preserves every field, while form helpers can land
// incrementally per kind without locking out advanced edits.

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { Dialog } from "@base-ui/react/dialog";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import {
  JsonEntityEditor,
  type JsonEntityEditorLabels,
} from "@/components/settings/vocabulary/json-entity-editor";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type {
  LinkMappingDef,
  ObjectMappingDef,
  OntologyEditOp,
} from "@/lib/api/edit-ops";
import { IntegrityIssuesBanner } from "@/components/ontology/integrity-issues-banner";
import {
  diagnosticHasParam,
  useOntologyValidation,
} from "@/hooks/api/use-ontology-validation";

const OBJECT_MAPPING_HINT = `{
  "id": "om-customer",
  "node_type_id": "node-customer",
  "source_id": "src-postgres",
  "relation": { "schema": "public", "table": "customers" },
  "primary_key_columns": [{ "name": "id" }],
  "property_mappings": []
}`;

const LINK_MAPPING_HINT = `{
  "id": "lm-customer-orders",
  "edge_type_id": "edge-placed",
  "kind": {
    "kind": "foreign_key",
    "source_column": { "name": "customer_id" },
    "target_column": { "name": "id" }
  },
  "cardinality": "OneToMany"
}`;

type MappingTab = "object" | "link";

/** Build the [`JsonEntityEditorLabels`] payload from the page's
 *  translation function for the given mapping kind. Hand-rolled
 *  here so the editor stays namespace-agnostic — same component
 *  serves rules / mappings / vocabulary surfaces by swapping
 *  `labels`. */
// next-intl's typed `Translator` is the canonical signature here;
// keeping the parameter open via `ReturnType<typeof useTranslations>`
// would re-introduce the same generic argument list. Inline the
// shape so the helper is callable from any
// `useTranslations("settings.vocabulary.mappings")` site.
type MappingsTranslator = (
  key: string,
  values?: Record<string, string | number | Date>,
) => string;

function mappingLabels(
  t: MappingsTranslator,
  kind: MappingTab,
): JsonEntityEditorLabels {
  return {
    title: t(kind === "object" ? "objectMapping" : "linkMapping"),
    jsonLabel: t("jsonLabel"),
    submitCreate: t("submitCreate"),
    submitUpdate: t("submitUpdate"),
    cancel: t("cancel"),
    errorEmpty: t("error.empty"),
    errorInvalidJsonTemplate: (message) =>
      t("error.invalidJson", { message }),
  };
}

export default function MappingsAdminPage() {
  const t = useTranslations("settings.vocabulary.mappings");
  const ontologiesQuery = useOntologies({ limit: 1 });
  const ontology = ontologiesQuery.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const [tab, setTab] = useState<MappingTab>("object");
  const [createOpen, setCreateOpen] = useState(false);
  const [editingObject, setEditingObject] = useState<ObjectMappingDef | null>(
    null,
  );
  const [editingLink, setEditingLink] = useState<LinkMappingDef | null>(null);

  // The FE-side OntologyIR doesn't enumerate every mapping field;
  // pull through an open record so the page reads the canonical
  // wire shape directly. Backend validation guarantees correctness.
  const ir = (detail.data?.ontology_ir ?? null) as Record<
    string,
    unknown
  > | null;
  const objectMappings = useMemo<ObjectMappingDef[]>(
    () => (ir?.object_mappings as ObjectMappingDef[] | undefined) ?? [],
    [ir],
  );
  const linkMappings = useMemo<LinkMappingDef[]>(
    () => (ir?.link_mappings as LinkMappingDef[] | undefined) ?? [],
    [ir],
  );
  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const submit = (operations: OntologyEditOp[], message: string) =>
    apply.mutateAsync({
      operations,
      expected_version: expectedVersion,
      message,
    });

  const handleCreateObject = async (def: ObjectMappingDef) => {
    if (!ontology?.id) return;
    try {
      await submit(
        [{ op: "create_object_mapping", mapping: def }],
        t("messages.objectCreated", {
          id: (def as { id?: string }).id ?? "?",
        }),
      );
      toast.success(t("toast.created"));
      setCreateOpen(false);
    } catch (err) {
      toast.error(t("toast.createFailed", { error: (err as Error).message }));
    }
  };

  const handleUpdateObject = async (def: ObjectMappingDef) => {
    if (!ontology?.id || !editingObject) return;
    const id = (editingObject as { id?: string }).id ?? "";
    try {
      await submit(
        [{ op: "update_object_mapping", id, mapping: def }],
        t("messages.objectUpdated", { id }),
      );
      toast.success(t("toast.updated"));
      setEditingObject(null);
    } catch (err) {
      toast.error(t("toast.updateFailed", { error: (err as Error).message }));
    }
  };

  const handleCreateLink = async (def: LinkMappingDef) => {
    if (!ontology?.id) return;
    try {
      await submit(
        [{ op: "create_link_mapping", mapping: def }],
        t("messages.linkCreated", {
          id: (def as { id?: string }).id ?? "?",
        }),
      );
      toast.success(t("toast.created"));
      setCreateOpen(false);
    } catch (err) {
      toast.error(t("toast.createFailed", { error: (err as Error).message }));
    }
  };

  const handleUpdateLink = async (def: LinkMappingDef) => {
    if (!ontology?.id || !editingLink) return;
    const id = (editingLink as { id?: string }).id ?? "";
    try {
      await submit(
        [{ op: "update_link_mapping", id, mapping: def }],
        t("messages.linkUpdated", { id }),
      );
      toast.success(t("toast.updated"));
      setEditingLink(null);
    } catch (err) {
      toast.error(t("toast.updateFailed", { error: (err as Error).message }));
    }
  };

  const handleDeleteObject = async (def: ObjectMappingDef) => {
    if (!ontology?.id) return;
    const id = (def as { id?: string }).id ?? "";
    const ok = await confirm({
      title: t("confirm.deleteTitle"),
      description: t("confirm.deleteDescription", { id }),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await submit(
        [{ op: "delete_object_mapping", id }],
        t("messages.objectDeleted", { id }),
      );
      toast.success(t("toast.deleted"));
    } catch (err) {
      toast.error(t("toast.deleteFailed", { error: (err as Error).message }));
    }
  };

  const handleDeleteLink = async (def: LinkMappingDef) => {
    if (!ontology?.id) return;
    const id = (def as { id?: string }).id ?? "";
    const ok = await confirm({
      title: t("confirm.deleteTitle"),
      description: t("confirm.deleteDescription", { id }),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await submit(
        [{ op: "delete_link_mapping", id }],
        t("messages.linkDeleted", { id }),
      );
      toast.success(t("toast.deleted"));
    } catch (err) {
      toast.error(t("toast.deleteFailed", { error: (err as Error).message }));
    }
  };

  if (ontologiesQuery.isLoading || detail.isLoading) {
    return (
      <div className="flex items-center justify-center py-10">
        <Spinner />
      </div>
    );
  }

  if (!ontology) {
    return (
      <div className="flex flex-col gap-4">
        <Header />
        <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
          {t("noOntology")}
        </p>
      </div>
    );
  }

  const items = tab === "object" ? objectMappings : linkMappings;

  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-start justify-between gap-4">
        <Header />
        <Button
          variant="primary"
          size="sm"
          onClick={() => setCreateOpen(true)}
          disabled={apply.isPending}
        >
          {t("createButton")}
        </Button>
      </header>

      <div className="flex gap-2 border-b border-zinc-200 dark:border-zinc-800">
        <TabButton
          active={tab === "object"}
          onClick={() => setTab("object")}
          label={t("tab.object")}
          count={objectMappings.length}
        />
        <TabButton
          active={tab === "link"}
          onClick={() => setTab("link")}
          label={t("tab.link")}
          count={linkMappings.length}
        />
      </div>

      {items.length === 0 ? (
        <EmptyState
          title={t(`empty.${tab}.title`)}
          description={t(`empty.${tab}.description`)}
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {tab === "object"
            ? objectMappings.map((m) => (
                <li
                  key={(m as { id: string }).id}
                  className="rounded border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-900"
                >
                  {editingObject?.id === (m as { id: string }).id ? (
                    <ObjectMappingEditor
                      mapping={m}
                      ontologyId={ontology?.id}
                      labels={mappingLabels(t, "object")}
                      onSubmit={handleUpdateObject}
                      onCancel={() => setEditingObject(null)}
                      pending={apply.isPending}
                    />
                  ) : (
                    <ObjectMappingRow
                      mapping={m}
                      onEdit={() => setEditingObject(m)}
                      onDelete={() => handleDeleteObject(m)}
                      busy={apply.isPending}
                    />
                  )}
                </li>
              ))
            : linkMappings.map((m) => (
                <li
                  key={(m as { id: string }).id}
                  className="rounded border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-900"
                >
                  {editingLink?.id === (m as { id: string }).id ? (
                    <LinkMappingEditor
                      mapping={m}
                      ontologyId={ontology?.id}
                      labels={mappingLabels(t, "link")}
                      onSubmit={handleUpdateLink}
                      onCancel={() => setEditingLink(null)}
                      pending={apply.isPending}
                    />
                  ) : (
                    <LinkMappingRow
                      mapping={m}
                      onEdit={() => setEditingLink(m)}
                      onDelete={() => handleDeleteLink(m)}
                      busy={apply.isPending}
                    />
                  )}
                </li>
              ))}
        </ul>
      )}

      <Dialog.Root open={createOpen} onOpenChange={setCreateOpen}>
        <Dialog.Portal>
          <Dialog.Backdrop className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm" />
          <Dialog.Popup
            className="fixed left-1/2 top-1/2 z-50 w-full max-w-2xl -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-xl border border-zinc-200 bg-white p-6 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
            style={{ maxHeight: "90vh" }}
          >
            <Dialog.Title className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
              {t(`createDialog.${tab}Title`)}
            </Dialog.Title>
            <Dialog.Description className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
              {t(`createDialog.${tab}Description`)}
            </Dialog.Description>
            <div className="mt-4">
              {tab === "object" ? (
                <JsonEntityEditor
                  schemaHint={OBJECT_MAPPING_HINT}
                  labels={mappingLabels(t, "object")}
                  onSubmit={handleCreateObject}
                  onCancel={() => setCreateOpen(false)}
                  pending={apply.isPending}
                />
              ) : (
                <JsonEntityEditor
                  schemaHint={LINK_MAPPING_HINT}
                  labels={mappingLabels(t, "link")}
                  onSubmit={handleCreateLink}
                  onCancel={() => setCreateOpen(false)}
                  pending={apply.isPending}
                />
              )}
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}

function Header() {
  const t = useTranslations("settings.vocabulary.mappings");
  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        {t("pageTitle")}
      </h1>
      <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
        {t("pageSubtitle")}
      </p>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  label,
  count,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  count: number;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        "border-b-2 px-2 pb-2 text-xs font-medium transition-colors " +
        (active
          ? "border-emerald-600 text-emerald-700 dark:border-emerald-400 dark:text-emerald-300"
          : "border-transparent text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200")
      }
    >
      {label} <span className="ml-1 text-muted-foreground">({count})</span>
    </button>
  );
}

function ObjectMappingRow({
  mapping,
  onEdit,
  onDelete,
  busy,
}: {
  mapping: ObjectMappingDef;
  onEdit: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  const t = useTranslations("settings.vocabulary.mappings");
  const m = mapping as Record<string, unknown>;
  const id = String(m.id ?? "?");
  const nodeTypeId = String(m.node_type_id ?? "?");
  const sourceId = String(m.source_id ?? "?");
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
            {id}
          </span>
          <span className="text-xs text-zinc-500 dark:text-zinc-400">
            → {nodeTypeId}
          </span>
          <span className="rounded bg-zinc-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
            {sourceId}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-1.5">
        <Button variant="ghost" size="xs" onClick={onEdit} disabled={busy}>
          {t("editButton")}
        </Button>
        <Button variant="ghost" size="xs" onClick={onDelete} disabled={busy}>
          {t("deleteButton")}
        </Button>
      </div>
    </div>
  );
}

function LinkMappingRow({
  mapping,
  onEdit,
  onDelete,
  busy,
}: {
  mapping: LinkMappingDef;
  onEdit: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  const t = useTranslations("settings.vocabulary.mappings");
  const m = mapping as Record<string, unknown>;
  const id = String(m.id ?? "?");
  const edgeTypeId = String(m.edge_type_id ?? "?");
  const kindObj = (m.kind as Record<string, unknown> | undefined) ?? {};
  const kindLabel = String(kindObj.kind ?? "?");
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
            {id}
          </span>
          <span className="text-xs text-zinc-500 dark:text-zinc-400">
            → {edgeTypeId}
          </span>
          <span className="rounded bg-blue-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">
            {kindLabel}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-1.5">
        <Button variant="ghost" size="xs" onClick={onEdit} disabled={busy}>
          {t("editButton")}
        </Button>
        <Button variant="ghost" size="xs" onClick={onDelete} disabled={busy}>
          {t("deleteButton")}
        </Button>
      </div>
    </div>
  );
}

function ObjectMappingEditor({
  mapping,
  ontologyId,
  labels,
  onSubmit,
  onCancel,
  pending,
}: {
  mapping: ObjectMappingDef;
  ontologyId: string | null | undefined;
  labels: JsonEntityEditorLabels;
  onSubmit: (def: ObjectMappingDef) => Promise<void> | void;
  onCancel: () => void;
  pending: boolean;
}) {
  const validation = useOntologyValidation(ontologyId);
  const issues = (validation.data ?? []).filter((d) =>
    diagnosticHasParam(d, "mapping_id", mapping.id),
  );
  return (
    <div className="space-y-2">
      <IntegrityIssuesBanner issues={issues} />
      <JsonEntityEditor
        key={mapping.id}
        initial={mapping}
        schemaHint={OBJECT_MAPPING_HINT}
        labels={labels}
        onSubmit={onSubmit}
        onCancel={onCancel}
        pending={pending}
      />
    </div>
  );
}

function LinkMappingEditor({
  mapping,
  ontologyId,
  labels,
  onSubmit,
  onCancel,
  pending,
}: {
  mapping: LinkMappingDef;
  ontologyId: string | null | undefined;
  labels: JsonEntityEditorLabels;
  onSubmit: (def: LinkMappingDef) => Promise<void> | void;
  onCancel: () => void;
  pending: boolean;
}) {
  const validation = useOntologyValidation(ontologyId);
  const issues = (validation.data ?? []).filter((d) =>
    diagnosticHasParam(d, "mapping_id", mapping.id),
  );
  return (
    <div className="space-y-2">
      <IntegrityIssuesBanner issues={issues} />
      <JsonEntityEditor
        key={mapping.id}
        initial={mapping}
        schemaHint={LINK_MAPPING_HINT}
        labels={labels}
        onSubmit={onSubmit}
        onCancel={onCancel}
        pending={pending}
      />
    </div>
  );
}

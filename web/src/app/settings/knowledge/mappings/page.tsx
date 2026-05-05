"use client";

// /settings/knowledge/mappings — master-detail CRUD page for the
// physical-to-logical mapping layer (ObjectMapping + LinkMapping).
//
// Both mapping kinds are edited through the typed schema-driven form
// (`StructuredEntityEditor`) so every field — discriminated
// LinkMappingKind variants, nested ColumnRef / EndpointRef compounds,
// PropertyMappingDef rows with their own location / transform branches
// — round-trips with full type fidelity. Industry pattern
// (Linear / Stripe / Sanity): list-pane + always-visible editor,
// modal reserved for destructive confirms.

import { useCallback, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";

import {
  DRAFT_ID,
  useMasterDetailSelection,
} from "@/hooks/use-master-detail-selection";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { FormInput } from "@/components/ui/form-input";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { SkeletonTable } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { useConfirm } from "@/components/providers/confirm-provider";
import { EntityWorkbench } from "@/components/workbench/entity-workbench";
import { IntegrityIssuesBanner } from "@/components/ontology/integrity-issues-banner";
import { StructuredEntityEditor } from "@/components/forms/structured-entity-editor";
import { linkMappingSchema } from "@/components/forms/schemas/link-mapping.schema";
import { objectMappingSchema } from "@/components/forms/schemas/object-mapping.schema";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import {
  diagnosticHasParam,
  useOntologyValidation,
} from "@/hooks/api/use-ontology-validation";
import type {
  LinkMappingDef,
  ObjectMappingDef,
  OntologyEditOp,
} from "@/lib/api/edit-ops";
import { cn } from "@/lib/cn";

type MappingTab = "object" | "link";
const TAB_PARAM = "kind";
const ID_PARAM = "id";

function mappingId(m: ObjectMappingDef | LinkMappingDef): string {
  return (m as { id?: string }).id ?? "";
}

export default function MappingsAdminPage() {
  const t = useTranslations("settings.knowledge.mappings");
  const tWorkbench = useTranslations("settings.vocabulary.workbench");
  const tCommon = useTranslations("common");
  const router = useRouter();
  const searchParams = useSearchParams();
  const detail = useWorkspaceOntology();
  const ontology = detail.data ?? null;
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const tab: MappingTab = searchParams.get(TAB_PARAM) === "link" ? "link" : "object";

  const setTab = useCallback(
    (next: MappingTab) => {
      const sp = new URLSearchParams(searchParams);
      sp.set(TAB_PARAM, next);
      sp.delete(ID_PARAM);
      router.replace(`?${sp.toString()}`);
    },
    [router, searchParams],
  );

  const ir = (detail.data?.ontology_ir ?? null) as Record<string, unknown> | null;
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

  const items: (ObjectMappingDef | LinkMappingDef)[] =
    tab === "object" ? objectMappings : linkMappings;
  // Selection round-trips through `?id=` — see hook for the
  // deep-link / draft-state contract. The hook auto-resets to
  // first-item whenever `items` changes (tab switch flips the
  // backing list, the auto-select effect re-fires).
  const { selectedId, selected, isDraft, setSelection } =
    useMasterDetailSelection({
      items,
      itemId: mappingId,
      selectionParam: ID_PARAM,
    });

  const [search, setSearch] = useState("");
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return items;
    return items.filter((m) => mappingId(m).toLowerCase().includes(needle));
  }, [items, search]);

  const submit = (operations: OntologyEditOp[], message: string) =>
    apply.mutateAsync({
      operations,
      expected_version: expectedVersion,
      message,
    });

  const handleCreateObject = async (def: ObjectMappingDef) => {
    if (!ontology?.id) return;
    const id = mappingId(def);
    try {
      await submit(
        [{ op: "create_object_mapping", mapping: def }],
        t("messages.objectCreated", { id: id || "?" }),
      );
      toast.success(t("toast.created"));
      setSelection(id);
    } catch (err) {
      toast.error(t("toast.createFailed", { error: (err as Error).message }));
    }
  };
  const handleUpdateObject = async (def: ObjectMappingDef) => {
    if (!ontology?.id || !selected) return;
    const id = mappingId(selected);
    try {
      await submit(
        [{ op: "update_object_mapping", id, mapping: def }],
        t("messages.objectUpdated", { id }),
      );
      toast.success(t("toast.updated"));
    } catch (err) {
      toast.error(t("toast.updateFailed", { error: (err as Error).message }));
    }
  };
  const handleCreateLink = async (def: LinkMappingDef) => {
    if (!ontology?.id) return;
    const id = mappingId(def);
    try {
      await submit(
        [{ op: "create_link_mapping", mapping: def }],
        t("messages.linkCreated", { id: id || "?" }),
      );
      toast.success(t("toast.created"));
      setSelection(id);
    } catch (err) {
      toast.error(t("toast.createFailed", { error: (err as Error).message }));
    }
  };
  const handleUpdateLink = async (def: LinkMappingDef) => {
    if (!ontology?.id || !selected) return;
    const id = mappingId(selected);
    try {
      await submit(
        [{ op: "update_link_mapping", id, mapping: def }],
        t("messages.linkUpdated", { id }),
      );
      toast.success(t("toast.updated"));
    } catch (err) {
      toast.error(t("toast.updateFailed", { error: (err as Error).message }));
    }
  };
  const handleDelete = async () => {
    if (!ontology?.id || !selected) return;
    const id = mappingId(selected);
    const ok = await confirm({
      title: t("confirm.deleteTitle"),
      description: t("confirm.deleteDescription", { id }),
      variant: "danger",
    });
    if (!ok) return;
    try {
      const op: OntologyEditOp =
        tab === "object"
          ? { op: "delete_object_mapping", id }
          : { op: "delete_link_mapping", id };
      await submit(
        [op],
        tab === "object"
          ? t("messages.objectDeleted", { id })
          : t("messages.linkDeleted", { id }),
      );
      toast.success(t("toast.deleted"));
      setSelection(null);
    } catch (err) {
      toast.error(t("toast.deleteFailed", { error: (err as Error).message }));
    }
  };

  const loading = detail.isLoading;
  const failed = detail.isError;
  const pageState: PageState = failed
    ? {
        kind: "error",
        onRetry: () => {
          detail.refetch();
        },
      }
    : loading
      ? { kind: "loading" }
      : !ontology
        ? { kind: "empty" }
        : { kind: "data" };

  return (
    <SettingsPageShell
      title={t("pageTitle")}
      subtitle={t("pageSubtitle")}
    >
      <PageStateView
        state={pageState}
        skeleton={<SkeletonTable rows={6} cols={4} />}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
        empty={{
          title: t("noOntology"),
        }}
      >
        <div className="flex gap-2 border-b border-divider">
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
        <div className="h-[calc(100vh-12rem)]">
          <EntityWorkbench<ObjectMappingDef | LinkMappingDef>
            listPane={
              <ListPane
                items={filtered}
                tab={tab}
                selectedId={selectedId}
                onSelect={setSelection}
                onCreate={() => setSelection(DRAFT_ID)}
                search={search}
                onSearch={setSearch}
                heading={t(`listHeading.${tab}`, { count: items.length })}
                createLabel={t("addLabel")}
                searchPlaceholder={tWorkbench("searchPlaceholder")}
                emptyTitle={t(`empty.${tab}.title`)}
                emptyDescription={t(`empty.${tab}.description`)}
                busy={apply.isPending}
              />
            }
            detailPane={
              <DetailPane
                key={isDraft ? "__draft__" : selected ? mappingId(selected) : "__empty__"}
                tab={tab}
                isDraft={isDraft}
                selected={selected}
                ontologyId={ontology?.id ?? null}
                deleteLabel={t("deleteButton")}
                draftTitle={t(`createDialog.${tab}Title`)}
                nothingTitle={tWorkbench("nothingSelected.title")}
                nothingDescription={tWorkbench("nothingSelected.description")}
                onCreateObject={handleCreateObject}
                onUpdateObject={handleUpdateObject}
                onCreateLink={handleCreateLink}
                onUpdateLink={handleUpdateLink}
                onDelete={handleDelete}
                onCancelDraft={() => setSelection(null)}
                pending={apply.isPending}
              />
            }
            selected={selected}
          />
        </div>
      </PageStateView>
    </SettingsPageShell>
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
      className={cn(
        "border-b-2 px-2 pb-2 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        active
          ? "border-brand-foreground text-brand-foreground-strong"
          : "border-transparent text-foreground-muted hover:text-foreground-subtle-strong",
      )}
    >
      {label} <span className="ms-1 text-foreground-muted">({count})</span>
    </button>
  );
}

interface ListPaneProps {
  items: (ObjectMappingDef | LinkMappingDef)[];
  tab: MappingTab;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  search: string;
  onSearch: (value: string) => void;
  heading: string;
  createLabel: string;
  searchPlaceholder: string;
  emptyTitle: string;
  emptyDescription: string;
  busy: boolean;
}

function ListPane({
  items,
  tab,
  selectedId,
  onSelect,
  onCreate,
  search,
  onSearch,
  heading,
  createLabel,
  searchPlaceholder,
  emptyTitle,
  emptyDescription,
  busy,
}: ListPaneProps) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-divider px-3 py-2">
        <h2 className="flex-1 text-xs font-semibold text-foreground-strong">
          {heading}
        </h2>
        <button
          type="button"
          onClick={onCreate}
          disabled={busy}
          aria-label={createLabel}
          className="inline-flex items-center gap-1 rounded bg-brand-solid px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-onbrand shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid disabled:opacity-50"
        >
          <span aria-hidden className="text-sm leading-none">
            +
          </span>
          {createLabel}
        </button>
      </div>
      <div className="border-b border-divider px-3 py-2">
        <FormInput
          type="search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder={searchPlaceholder}
          density="compact"
        />
      </div>
      {items.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-4 py-6">
          <EmptyState title={emptyTitle} description={emptyDescription} />
        </div>
      ) : (
        <ul className="flex-1 overflow-y-auto py-1">
          {items.map((m) => {
            const id = mappingId(m);
            const isSelected = id === selectedId;
            return (
              <li key={id}>
                <button
                  type="button"
                  onClick={() => onSelect(id)}
                  className={cn(
                    "flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left text-xs transition-colors",
                    isSelected
                      ? "bg-brand-surface text-brand-foreground"
                      : "text-foreground hover:bg-surface-hover",
                  )}
                >
                  {tab === "object" ? (
                    <ObjectRowBody mapping={m as ObjectMappingDef} />
                  ) : (
                    <LinkRowBody mapping={m as LinkMappingDef} />
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function ObjectRowBody({ mapping }: { mapping: ObjectMappingDef }) {
  const m = mapping as Record<string, unknown>;
  return (
    <>
      <span className="font-mono text-2xs font-medium">{String(m.id ?? "?")}</span>
      <span className="flex flex-wrap items-center gap-1.5 text-2xs text-foreground-muted">
        <span>→ {String(m.node_type_id ?? "?")}</span>
        <span className="rounded bg-surface-inset px-1.5 py-0.5 uppercase">
          {String(m.source_id ?? "?")}
        </span>
      </span>
    </>
  );
}

function LinkRowBody({ mapping }: { mapping: LinkMappingDef }) {
  const m = mapping as Record<string, unknown>;
  const kindObj = (m.kind as Record<string, unknown> | undefined) ?? {};
  return (
    <>
      <span className="font-mono text-2xs font-medium">{String(m.id ?? "?")}</span>
      <span className="flex flex-wrap items-center gap-1.5 text-2xs text-foreground-muted">
        <span>→ {String(m.edge_type_id ?? "?")}</span>
        <span className="rounded bg-info-surface px-1.5 py-0.5 uppercase text-info-foreground">
          {String(kindObj.kind ?? "?")}
        </span>
      </span>
    </>
  );
}

interface DetailPaneProps {
  tab: MappingTab;
  isDraft: boolean;
  selected: ObjectMappingDef | LinkMappingDef | null;
  ontologyId: string | null;
  deleteLabel: string;
  draftTitle: string;
  nothingTitle: string;
  nothingDescription: string;
  onCreateObject: (def: ObjectMappingDef) => Promise<void>;
  onUpdateObject: (def: ObjectMappingDef) => Promise<void>;
  onCreateLink: (def: LinkMappingDef) => Promise<void>;
  onUpdateLink: (def: LinkMappingDef) => Promise<void>;
  onDelete: () => Promise<void>;
  onCancelDraft: () => void;
  pending: boolean;
}

function DetailPane({
  tab,
  isDraft,
  selected,
  ontologyId,
  deleteLabel,
  draftTitle,
  nothingTitle,
  nothingDescription,
  onCreateObject,
  onUpdateObject,
  onCreateLink,
  onUpdateLink,
  onDelete,
  onCancelDraft,
  pending,
}: DetailPaneProps) {
  const validation = useOntologyValidation(ontologyId);
  const issues =
    selected && !isDraft
      ? (validation.data ?? []).filter((d) =>
          diagnosticHasParam(d, "mapping_id", mappingId(selected)),
        )
      : [];

  if (!isDraft && !selected) {
    return (
      <div className="flex h-full items-center justify-center px-6 py-12">
        <EmptyState title={nothingTitle} description={nothingDescription} />
      </div>
    );
  }
  const title = isDraft ? draftTitle : selected ? mappingId(selected) : "";

  return (
    <div className="flex h-full min-w-0 flex-col">
      <header className="flex items-center gap-3 border-b border-divider px-4 py-3">
        <h2 className="flex-1 truncate font-mono text-sm font-semibold text-foreground-strong">
          {title}
        </h2>
        {!isDraft && selected && (
          <Button
            variant="danger"
            size="sm"
            onClick={onDelete}
            disabled={pending}
          >
            {deleteLabel}
          </Button>
        )}
      </header>
      <div className="flex-1 space-y-2 overflow-y-auto px-4 py-4">
        {!isDraft && issues.length > 0 && (
          <IntegrityIssuesBanner issues={issues} />
        )}
        {tab === "object" ? (
          <StructuredEntityEditor<ObjectMappingDef>
            schema={objectMappingSchema}
            initial={
              isDraft ? undefined : (selected as ObjectMappingDef) ?? undefined
            }
            onSubmit={isDraft ? onCreateObject : onUpdateObject}
            onCancel={isDraft ? onCancelDraft : undefined}
            pending={pending}
          />
        ) : (
          <StructuredEntityEditor<LinkMappingDef>
            schema={linkMappingSchema}
            initial={
              isDraft ? undefined : (selected as LinkMappingDef) ?? undefined
            }
            onSubmit={isDraft ? onCreateLink : onUpdateLink}
            onCancel={isDraft ? onCancelDraft : undefined}
            pending={pending}
          />
        )}
      </div>
    </div>
  );
}

"use client";

import { useCallback, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";

import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { BookOpen01Icon } from "@hugeicons/core-free-icons";
import { useConfirm } from "@/components/providers/confirm-provider";
import { GlossaryForm } from "@/components/vocabulary/glossary-form";
import { ResolutionModal } from "@/components/ambiguity/resolution-modal";
import { GlossaryBindingPanel } from "@/components/glossary/glossary-binding-panel";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import {
  useAmbiguity,
  useResolveAmbiguity,
} from "@/hooks/api/use-ambiguities";
import type { AmbiguityMapping } from "@/lib/api/ambiguity";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import { arr } from "@/lib/ir-collections";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";
import type { OntologyIR } from "@/types/api";

import { TermTree, type TermAnchorCounts } from "./term-tree";
import { UsageMap } from "./usage-map";

// ---------------------------------------------------------------------------
// GlossaryWorkbench — 3-pane workbench mode for the canonical
// vocabulary layer (`GlossaryTermDef` + optional `TermRealisation`).
// Cross-cutting: every other workspace mode reads it (Design
// anchors, Analyze chips, Explore semantic labels), so admin-gating
// it behind Settings would be an asymmetry. Single-source
// data flow: ontology snapshot from `useOntologyDetail` →
// `OntologyIR.glossary` drives both the tree and the usage map; the
// editor batches mutations through `useApplyOntologyEdits` so
// audit + version commit ride the same `/edits` pipeline as
// Design-mode changes.
// ---------------------------------------------------------------------------

const TERM_PARAM = "term";
const AMBIGUITY_PARAM = "ambiguity";
const ROUTE = "/glossary";

function freshGlossaryId(): string {
  return `gt-${crypto.randomUUID()}`;
}

/// Pure URL helper. Centralised here so the workbench never builds
/// `URLSearchParams` ad-hoc — every navigation routes through this
/// shape and stays consistent.
function buildHref(
  current: URLSearchParams,
  patches: Record<string, string | null>,
): string {
  const next = new URLSearchParams(current);
  for (const [key, value] of Object.entries(patches)) {
    if (value === null) next.delete(key);
    else next.set(key, value);
  }
  const qs = next.toString();
  return qs ? `${ROUTE}?${qs}` : ROUTE;
}

function computeAnchorCounts(ontology: OntologyIR): TermAnchorCounts {
  const byTermId = new Map<string, number>();
  const bump = (id: string) => {
    byTermId.set(id, (byTermId.get(id) ?? 0) + 1);
  };
  for (const node of arr(ontology.node_types)) {
    for (const anchor of arr(node.glossary_anchors)) bump(anchor);
    for (const property of arr(node.properties)) {
      for (const binding of arr(property.bindings)) {
        if (binding.kind === "glossary") bump(binding.id);
      }
    }
  }
  for (const edge of arr(ontology.edge_types)) {
    for (const anchor of arr(edge.glossary_anchors)) bump(anchor);
    for (const property of arr(edge.properties)) {
      for (const binding of arr(property.bindings)) {
        if (binding.kind === "glossary") bump(binding.id);
      }
    }
  }
  return { byTermId };
}

export function GlossaryWorkbench() {
  const t = useTranslations("workbench.glossary");
  const tCommon = useTranslations("common");
  const tForm = useTranslations("settings.vocabulary.glossary");
  const localeChain = useLocaleChain();
  const router = useRouter();
  const searchParams = useSearchParams();
  const confirm = useConfirm();

  // Workbench owns the ontology fetch directly: this surface stands
  // on its own without a project (analogous to Analyze / Explore),
  // so we load the latest committed ontology by listing top-1 +
  // detail. The detail snapshot is the single source of truth for
  // every pane below; we never mirror it into the global Zustand
  // cache because each consumer that needs an ontology fetches its
  // own (avoids two-source drift between fetch and cache).
  const ontologyDetailQuery = useWorkspaceOntology();
  const ontologyMeta = ontologyDetailQuery.data ?? null;
  const apply = useApplyOntologyEdits(ontologyMeta?.id);

  const ontology = ontologyDetailQuery.data?.ontology_ir as
    | OntologyIR
    | undefined;
  const expectedVersion =
    Number(ontologyDetailQuery.data?.current_version?.version ?? "0") || 0;

  const glossary: readonly GlossaryTermDef[] = useMemo(
    () => (ontology ? arr(ontology.glossary) : []),
    [ontology],
  );

  const anchorCounts = useMemo(
    () => (ontology ? computeAnchorCounts(ontology) : { byTermId: new Map() }),
    [ontology],
  );

  // Selection lives in the URL so deep links (`?term=g-…`) and
  // chat-panel disambiguation chips round-trip cleanly. Falls back
  // to the first term so the editor pane always renders something.
  const urlTermId = searchParams.get(TERM_PARAM);
  const [draftCreate, setDraftCreate] = useState(false);
  const selectedTermId =
    urlTermId && glossary.some((g) => g.id === urlTermId)
      ? urlTermId
      : glossary[0]?.id ?? null;
  const selectedTerm = glossary.find((g) => g.id === selectedTermId) ?? null;

  const setSelectedTermId = useCallback(
    (id: string | null) => {
      setDraftCreate(false);
      router.replace(buildHref(searchParams, { [TERM_PARAM]: id }));
    },
    [router, searchParams],
  );

  const dismissAmbiguityHint = useCallback(() => {
    router.replace(buildHref(searchParams, { [AMBIGUITY_PARAM]: null }));
  }, [router, searchParams]);

  // ------------------------------------------------------------------
  // Mutations — matched to the existing `/edits` op surface. Each
  // submit builds one op + locks the form via `apply.isPending`.
  // ------------------------------------------------------------------

  const handleCreate = (def: GlossaryTermDef) => {
    if (!ontologyMeta?.id) return;
    const label = localize(def.term, localeChain);
    const id = def.id || freshGlossaryId();
    apply.mutate(
      {
        operations: [
          { op: "create_glossary_term", def: { ...def, id } },
        ],
        expected_version: expectedVersion,
        message: tForm("messages.created", { term: label }),
      },
      {
        onSuccess: () => {
          toast.success(tForm("toast.created", { term: label }));
          // setSelectedTermId clears the create draft + lands the
          // URL on the new term in one go.
          setSelectedTermId(id);
        },
        onError: (err) =>
          toast.error(tForm("toast.createFailed", { error: err.message })),
      },
    );
  };

  const handleUpdate = (def: GlossaryTermDef) => {
    if (!ontologyMeta?.id || !def.id) return;
    const label = localize(def.term, localeChain);
    apply.mutate(
      {
        operations: [{ op: "update_glossary_term", id: def.id, def }],
        expected_version: expectedVersion,
        message: tForm("messages.updated", { term: label }),
      },
      {
        onSuccess: () => toast.success(tForm("toast.updated", { term: label })),
        onError: (err) =>
          toast.error(tForm("toast.updateFailed", { error: err.message })),
      },
    );
  };

  const handleDelete = async () => {
    if (!ontologyMeta?.id || !selectedTerm) return;
    const label = localize(selectedTerm.term, localeChain);
    const ok = await confirm({
      title: tForm("confirm.deleteTitle"),
      description: tForm("confirm.deleteDescription", { term: label }),
      confirmLabel: tForm("confirm.deleteConfirm"),
      cancelLabel: tForm("confirm.cancel"),
      variant: "danger",
    });
    if (!ok) return;
    apply.mutate(
      {
        operations: [{ op: "delete_glossary_term", id: selectedTerm.id }],
        expected_version: expectedVersion,
        message: tForm("messages.deleted", { term: label }),
      },
      {
        onSuccess: () => {
          toast.success(tForm("toast.deleted", { term: label }));
          setSelectedTermId(null);
        },
        onError: (err) =>
          toast.error(tForm("toast.deleteFailed", { error: err.message })),
      },
    );
  };

  // ------------------------------------------------------------------
  // Render branches
  // ------------------------------------------------------------------

  if (ontologyDetailQuery.isLoading) {
    return (
      <WorkbenchPageShell title={t("heading")} subtitle={t("subtitle")}>
        <div className="px-6 py-6">
          <SkeletonList count={6} />
        </div>
      </WorkbenchPageShell>
    );
  }

  if (ontologyDetailQuery.isError) {
    return (
      <WorkbenchPageShell title={t("heading")} subtitle={t("subtitle")}>
        <div className="flex h-full items-center justify-center px-6 py-12">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => ontologyDetailQuery.refetch()}
            retryLabel={tCommon("retry")}
          />
        </div>
      </WorkbenchPageShell>
    );
  }

  if (!ontology || !ontologyMeta) {
    return (
      <WorkbenchPageShell title={t("heading")} subtitle={t("subtitle")}>
        <div className="flex h-full items-center justify-center px-6 py-12">
          <EmptyState
            icon={BookOpen01Icon}
            title={t("noOntology.title")}
            description={t("noOntology.description")}
          />
        </div>
      </WorkbenchPageShell>
    );
  }

  const ambiguityContextId = searchParams.get(AMBIGUITY_PARAM);

  return (
    <div className="flex h-full flex-col overflow-hidden bg-surface-base">
      {ambiguityContextId && (
        <AmbiguityResolutionBanner
          contextId={ambiguityContextId}
          onDismiss={dismissAmbiguityHint}
        />
      )}
      <div className="grid min-h-0 flex-1 grid-cols-[280px_minmax(0,1fr)_340px] divide-x divide-divider">
        <TermTree
        terms={glossary}
        selectedTermId={selectedTermId}
        onSelect={setSelectedTermId}
        onCreate={() => {
          setDraftCreate(true);
        }}
        anchorCounts={anchorCounts}
      />

      <div className="flex h-full min-w-0 flex-col overflow-hidden">
        {draftCreate ? (
          <EditorPane
            key="create"
            mode="create"
            availableTerms={glossary}
            onSubmit={handleCreate}
            onCancel={() => setDraftCreate(false)}
            pending={apply.isPending}
            title={t("editor.createTitle")}
          />
        ) : selectedTerm ? (
          <EditorPane
            key={`edit-${selectedTerm.id}`}
            mode="edit"
            initial={selectedTerm}
            availableTerms={glossary}
            onSubmit={handleUpdate}
            onCancel={() => undefined}
            onDelete={handleDelete}
            pending={apply.isPending}
            title={localize(selectedTerm.term, localeChain)}
          />
        ) : (
          <div className="flex h-full items-center justify-center px-6 py-12">
            <EmptyState
              title={t("editor.empty.title")}
              description={t("editor.empty.description")}
            />
          </div>
        )}
      </div>

        <div className="h-full min-w-0 overflow-hidden">
          {selectedTerm && !draftCreate ? (
            <RightPane
              ontology={ontology}
              ontologyId={ontologyMeta.id}
              expectedVersion={expectedVersion}
              term={selectedTerm}
            />
          ) : (
            <div className="flex h-full flex-col items-center justify-center px-4 py-8 text-center">
              <p className="text-2xs text-foreground-muted">
                {t("usage.placeholder")}
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

type RightPaneTab = "usage" | "bindings";

function RightPane({
  ontology,
  ontologyId,
  expectedVersion,
  term,
}: {
  ontology: OntologyIR;
  ontologyId: string;
  expectedVersion: number;
  term: GlossaryTermDef;
}) {
  const t = useTranslations("workbench.glossary.rightPane");
  const [tab, setTab] = useState<RightPaneTab>("usage");

  const termContext = useMemo(
    () => ({
      term_id: term.id,
      term: term.term,
      aliases: arr(term.aliases),
      description: term.description,
    }),
    [term],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <nav
        aria-label={t("tabsAria")}
        className="flex shrink-0 gap-1 border-b border-divider px-2"
      >
        {(["usage", "bindings"] as const).map((k) => (
          <button
            key={k}
            type="button"
            onClick={() => setTab(k)}
            aria-pressed={tab === k}
            className={`relative px-2.5 py-2 text-2xs font-medium ${
              tab === k
                ? "text-brand-foreground"
                : "text-foreground-muted hover:text-foreground-muted"
            }`}
          >
            {t(`tabs.${k}`)}
            {tab === k && (
              <span className="absolute inset-x-0 -bottom-px h-0.5 bg-brand-solid" />
            )}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-hidden">
        {tab === "usage" ? (
          <UsageMap ontology={ontology} termId={term.id} />
        ) : (
          <div className="h-full overflow-hidden p-3">
            <GlossaryBindingPanel
              ontologyId={ontologyId}
              expectedVersion={expectedVersion}
              term={termContext}
            />
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// EditorPane — wraps `GlossaryForm` with a header strip carrying the
// term label, lifecycle hint, and (when editing) a delete button.
// `mode` controls submit-button copy; `initial` drives the form on
// edit. The form remounts via `key=` on the parent so we don't sync
// `initial` with `useEffect` (per react-hooks/set-state-in-effect).
// ---------------------------------------------------------------------------

function EditorPane({
  mode,
  initial,
  availableTerms,
  onSubmit,
  onCancel,
  onDelete,
  pending,
  title,
}: {
  mode: "create" | "edit";
  initial?: GlossaryTermDef;
  availableTerms: readonly GlossaryTermDef[];
  onSubmit: (def: GlossaryTermDef) => void;
  onCancel: () => void;
  onDelete?: () => void;
  pending: boolean;
  title: string;
}) {
  const t = useTranslations("workbench.glossary.editor");
  const lifecycleState = initial?.lifecycle?.state ?? "active";
  const isInactive = lifecycleState !== "active";

  return (
    <>
      <header className="flex items-center gap-3 border-b border-divider px-4 py-3">
        <span className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-bold uppercase text-brand-foreground-strong">
          {t(mode === "create" ? "badges.create" : "badges.term")}
        </span>
        <h2 className="flex-1 truncate text-sm font-semibold text-foreground-strong">
          {title}
        </h2>
        {mode === "edit" && isInactive && (
          <span className="rounded bg-warning-surface px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-warning-foreground">
            {t(`lifecycle.${lifecycleState}`)}
          </span>
        )}
        {mode === "edit" && onDelete && (
          <button
            type="button"
            onClick={onDelete}
            disabled={pending}
            className="rounded border border-danger-border px-2.5 py-1 text-2xs font-medium text-danger-foreground hover:bg-danger-surface disabled:opacity-50"
          >
            {t("deleteAction")}
          </button>
        )}
      </header>
      <div className="flex-1 overflow-y-auto px-4 py-4">
        <GlossaryForm
          initial={initial}
          availableTerms={[...availableTerms]}
          onSubmit={onSubmit}
          onCancel={onCancel}
          pending={pending}
        />
      </div>
    </>
  );
}

function AmbiguityResolutionBanner({
  contextId,
  onDismiss,
}: {
  contextId: string;
  onDismiss: () => void;
}) {
  const t = useTranslations("workbench.glossary.ambiguityBanner");
  const [modalOpen, setModalOpen] = useState(false);

  const ambiguityQuery = useAmbiguity(contextId);
  const resolve = useResolveAmbiguity({
    onSuccess: () => {
      setModalOpen(false);
      onDismiss();
    },
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : String(err)),
  });

  const context = ambiguityQuery.data?.context ?? null;
  const activeResolution = useMemo(() => {
    const history = ambiguityQuery.data?.history ?? [];
    // First non-revoked entry, latest-first by `resolved_at`.
    return (
      [...history]
        .filter((r) => !r.revoked_at)
        .sort((a, b) =>
          b.resolved_at.localeCompare(a.resolved_at),
        )[0] ?? null
    );
  }, [ambiguityQuery.data]);

  return (
    <>
      <div className="flex items-start gap-3 border-b border-warning-border bg-warning-surface px-4 py-2.5 text-xs">
        <div className="flex-1">
          <p className="font-medium text-warning-foreground">
            {t("title")}
          </p>
          <p className="mt-0.5 text-warning-foreground">
            {t("description")}
          </p>
          {ambiguityQuery.isLoading && (
            <p className="mt-1 text-2xs text-warning-foreground">
              {t("loading")}
            </p>
          )}
          {ambiguityQuery.isError && (
            <p className="mt-1 text-2xs text-warning-foreground">
              {t("loadFailed")}
            </p>
          )}
          {context && (
            <p className="mt-1 font-mono text-2xs text-warning-foreground">
              {t("columnLabel")}: {context.column.relation}.
              {context.column.column}
            </p>
          )}
          {!context && (
            <p className="mt-1 font-mono text-2xs text-warning-foreground">
              {t("contextLabel")}: {contextId}
            </p>
          )}
        </div>
        {context && (
          <button
            type="button"
            onClick={() => setModalOpen(true)}
            className="rounded bg-warning-foreground px-2.5 py-1 text-2xs font-medium text-foreground-onbrand hover:bg-warning-foreground"
          >
            {t("resolve")}
          </button>
        )}
        <button
          type="button"
          onClick={onDismiss}
          className="rounded p-1 text-warning-foreground hover:bg-warning-surface"
          aria-label={t("dismissAria")}
        >
          ✕
        </button>
      </div>
      {context && modalOpen && (
        <ResolutionModal
          context={context}
          active={activeResolution}
          busy={resolve.isPending}
          onCancel={() => setModalOpen(false)}
          onSubmit={(mapping: AmbiguityMapping) => {
            resolve.mutate({ id: context.id, mapping });
          }}
        />
      )}
    </>
  );
}

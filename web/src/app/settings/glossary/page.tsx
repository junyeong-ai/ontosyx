"use client";

// /settings/glossary — canonical CRUD page for `GlossaryTermDef`.
// Φ4 deliverable. Reads the glossary slice of the workspace's
// current ontology, surfaces a list with create / edit / delete
// affordances, and submits batched OntologyEditOps through
// `useApplyOntologyEdits`. Every mutation flows through the
// `/api/ontologies/{id}/edits` pipeline, so rule-validation,
// version commits, and audit-log entries land on the same path the
// designer-side admin API uses.

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { Dialog } from "@base-ui/react/dialog";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { GlossaryForm } from "@/components/settings/vocabulary/glossary-form";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";

// ---------------------------------------------------------------------------
// id minting — fresh GlossaryTermId for create flows.
//
// Backend accepts an empty `id` on create (UUID auto-generates) but
// the front-end pre-generates so optimistic UI can show the new row
// immediately without waiting for the server's echo. UUID v4 via the
// browser-built-in is sufficient — the IR-side identity check uses
// the string verbatim.
// ---------------------------------------------------------------------------
function freshGlossaryId(): string {
  return `gt-${crypto.randomUUID()}`;
}

export default function GlossaryAdminPage() {
  const t = useTranslations("settings.vocabulary.glossary");
  const ontologiesQuery = useOntologies({ limit: 1 });
  const ontology = ontologiesQuery.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const [createOpen, setCreateOpen] = useState(false);
  const [editing, setEditing] = useState<GlossaryTermDef | null>(null);

  const glossary: GlossaryTermDef[] = useMemo(() => {
    return (detail.data?.ontology_ir?.glossary ?? []) as GlossaryTermDef[];
  }, [detail.data]);

  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  // ------------------------------------------------------------------
  // Mutation handlers
  // ------------------------------------------------------------------

  const handleCreate = (def: GlossaryTermDef) => {
    if (!ontology?.id) return;
    apply.mutate(
      {
        operations: [
          {
            op: "create_glossary_term",
            def: { ...def, id: def.id || freshGlossaryId() },
          },
        ],
        expected_version: expectedVersion,
        message: t("messages.created", { term: def.term }),
      },
      {
        onSuccess: () => {
          toast.success(t("toast.created", { term: def.term }));
          setCreateOpen(false);
        },
        onError: (err) => {
          toast.error(t("toast.createFailed", { error: err.message }));
        },
      },
    );
  };

  const handleUpdate = (def: GlossaryTermDef) => {
    if (!ontology?.id || !def.id) return;
    apply.mutate(
      {
        operations: [{ op: "update_glossary_term", id: def.id, def }],
        expected_version: expectedVersion,
        message: t("messages.updated", { term: def.term }),
      },
      {
        onSuccess: () => {
          toast.success(t("toast.updated", { term: def.term }));
          setEditing(null);
        },
        onError: (err) => {
          toast.error(t("toast.updateFailed", { error: err.message }));
        },
      },
    );
  };

  const handleDelete = async (term: GlossaryTermDef) => {
    if (!ontology?.id) return;
    const ok = await confirm({
      title: t("confirm.deleteTitle"),
      description: t("confirm.deleteDescription", { term: term.term }),
      confirmLabel: t("confirm.deleteConfirm"),
      cancelLabel: t("confirm.cancel"),
      variant: "danger",
    });
    if (!ok) return;
    apply.mutate(
      {
        operations: [{ op: "delete_glossary_term", id: term.id }],
        expected_version: expectedVersion,
        message: t("messages.deleted", { term: term.term }),
      },
      {
        onSuccess: () => toast.success(t("toast.deleted", { term: term.term })),
        onError: (err) =>
          toast.error(t("toast.deleteFailed", { error: err.message })),
      },
    );
  };

  // ------------------------------------------------------------------
  // Render
  // ------------------------------------------------------------------

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
        <header>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {t("pageTitle")}
          </h1>
          <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
            {t("pageSubtitle")}
          </p>
        </header>
        <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
          {t("noOntology")}
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {t("pageTitle")}
          </h1>
          <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
            {t("pageSubtitle")}
          </p>
        </div>
        <Button
          variant="primary"
          size="sm"
          onClick={() => setCreateOpen(true)}
          disabled={apply.isPending}
        >
          {t("createButton")}
        </Button>
      </header>

      {glossary.length === 0 ? (
        <EmptyState
          title={t("empty.title")}
          description={t("empty.description")}
        />
      ) : (
        <ul className="flex flex-col gap-2" data-testid="glossary-list">
          {glossary.map((term) => (
            <li
              key={term.id}
              className="rounded border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-900"
            >
              {editing?.id === term.id ? (
                <GlossaryForm
                  key={term.id}
                  initial={term}
                  onSubmit={handleUpdate}
                  onCancel={() => setEditing(null)}
                  pending={apply.isPending}
                />
              ) : (
                <GlossaryRow
                  term={term}
                  onEdit={() => setEditing(term)}
                  onDelete={() => handleDelete(term)}
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
          <Dialog.Popup className="fixed left-1/2 top-1/2 z-50 w-full max-w-lg -translate-x-1/2 -translate-y-1/2 rounded-xl border border-zinc-200 bg-white p-6 shadow-xl dark:border-zinc-700 dark:bg-zinc-900">
            <Dialog.Title className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
              {t("createDialog.title")}
            </Dialog.Title>
            <Dialog.Description className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
              {t("createDialog.description")}
            </Dialog.Description>
            <div className="mt-4">
              <GlossaryForm
                onSubmit={handleCreate}
                onCancel={() => setCreateOpen(false)}
                pending={apply.isPending}
              />
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}

// ---------------------------------------------------------------------------
// GlossaryRow — read-only display of one term + edit / delete buttons.
// Extracted so the same component renders inside both the list and
// (eventually) a search-results panel.
// ---------------------------------------------------------------------------
function GlossaryRow({
  term,
  onEdit,
  onDelete,
  busy,
}: {
  term: GlossaryTermDef;
  onEdit: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  const t = useTranslations("settings.vocabulary.glossary");
  const aliases = term.aliases ?? [];
  const displayName = term.display_name?.default;
  const description = term.description?.default;

  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
            {term.term}
          </span>
          {displayName && displayName !== term.term && (
            <span className="text-xs text-zinc-500 dark:text-zinc-400">
              · {displayName}
            </span>
          )}
          {term.category && (
            <span className="rounded bg-zinc-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
              {term.category}
            </span>
          )}
        </div>
        {description && (
          <p className="mt-1 text-xs text-zinc-600 dark:text-zinc-400">
            {description}
          </p>
        )}
        {aliases.length > 0 && (
          <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
            {t("aliasesLabel")}: {aliases.join(", ")}
          </p>
        )}
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

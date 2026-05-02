"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Modal } from "@/components/ui/modal";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/providers/confirm-provider";
import { RuleForm } from "@/components/vocabulary/rule-form";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp, RuleDef } from "@/lib/api/edit-ops";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

function freshRuleId(): string {
  return `rule-${crypto.randomUUID().slice(0, 8)}`;
}

export function RulesTab() {
  const t = useTranslations("settings.vocabulary.rules");
  const ontologiesQuery = useOntologies({ limit: 1 });
  const ontology = ontologiesQuery.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const [createOpen, setCreateOpen] = useState(false);
  const [editing, setEditing] = useState<RuleDef | null>(null);

  const rules = useMemo<RuleDef[]>(
    () => (detail.data?.ontology_ir?.rules as RuleDef[] | undefined) ?? [],
    [detail.data?.ontology_ir?.rules],
  );
  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const submit = (operations: OntologyEditOp[], message: string) =>
    apply.mutateAsync({
      operations,
      expected_version: expectedVersion,
      message,
    });

  const handleCreate = async (def: RuleDef) => {
    if (!ontology?.id) return;
    const withId: RuleDef = {
      ...def,
      id: def.id || freshRuleId(),
    };
    try {
      await submit(
        [{ op: "create_rule", def: withId }],
        t("messages.created", { id: withId.id }),
      );
      toast.success(t("toast.created", { id: withId.id }));
      setCreateOpen(false);
    } catch (err) {
      toast.error(
        t("toast.createFailed", { error: (err as Error).message }),
      );
    }
  };

  const handleUpdate = async (def: RuleDef) => {
    if (!ontology?.id || !editing) return;
    try {
      await submit(
        [{ op: "update_rule", id: editing.id, def }],
        t("messages.updated", { id: editing.id }),
      );
      toast.success(t("toast.updated", { id: editing.id }));
      setEditing(null);
    } catch (err) {
      toast.error(
        t("toast.updateFailed", { error: (err as Error).message }),
      );
    }
  };

  const handleDelete = async (rule: RuleDef) => {
    if (!ontology?.id) return;
    const ok = await confirm({
      title: t("confirm.deleteTitle"),
      description: t("confirm.deleteDescription", { name: rule.id }),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await submit(
        [{ op: "delete_rule", id: rule.id }],
        t("messages.deleted", { name: rule.id }),
      );
      toast.success(t("toast.deleted", { name: rule.id }));
    } catch (err) {
      toast.error(
        t("toast.deleteFailed", { error: (err as Error).message }),
      );
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
        <p className="rounded border border-warning-border bg-warning-surface p-3 text-xs text-warning-foreground">
          {t("noOntology")}
        </p>
      </div>
    );
  }

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

      {rules.length === 0 ? (
        <EmptyState
          title={t("empty.title")}
          description={t("empty.description")}
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {rules.map((rule) => (
            <li
              key={rule.id}
              className="rounded border border-divider bg-surface-base p-3"
            >
              {editing?.id === rule.id ? (
                <RuleForm
                  initial={rule}
                  ontologyId={ontology?.id}
                  onSubmit={handleUpdate}
                  onCancel={() => setEditing(null)}
                  pending={apply.isPending}
                />
              ) : (
                <RuleRow
                  rule={rule}
                  onEdit={() => setEditing(rule)}
                  onDelete={() => handleDelete(rule)}
                  busy={apply.isPending}
                />
              )}
            </li>
          ))}
        </ul>
      )}

      <Modal
        open={createOpen}
        onOpenChange={setCreateOpen}
        title={t("createDialog.title")}
        description={t("createDialog.description")}
        size="lg"
      >
        <RuleForm
          onSubmit={handleCreate}
          onCancel={() => setCreateOpen(false)}
          pending={apply.isPending}
        />
      </Modal>
    </div>
  );
}

function Header() {
  const t = useTranslations("settings.vocabulary.rules");
  return (
    <div>
      <h1 className="text-xl font-semibold text-foreground-strong">
        {t("pageTitle")}
      </h1>
      <p className="mt-1 text-xs text-foreground-muted">
        {t("pageSubtitle")}
      </p>
    </div>
  );
}

function RuleRow({
  rule,
  onEdit,
  onDelete,
  busy,
}: {
  rule: RuleDef;
  onEdit: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  const t = useTranslations("settings.vocabulary.rules");
  const localeChain = useLocaleChain();
  const name = rule.name ? localizePresent(rule.name, localeChain) : null;
  const isDerived = rule.origin?.kind === "derived_from_binding";

  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-sm font-medium text-foreground-strong">
            {rule.id}
          </span>
          {name && name !== rule.id && (
            <span className="text-xs text-foreground-muted">· {name}</span>
          )}
          {rule.severity && (
            <span
              className={`rounded px-2 py-0.5 text-2xs font-medium uppercase tracking-wider ${severityClass(rule.severity)}`}
            >
              {t(`severity.${rule.severity}`)}
            </span>
          )}
          {rule.enforcement && (
            <span className="rounded bg-surface-inset px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-foreground-muted">
              {t(`enforcement.${rule.enforcement}`)}
            </span>
          )}
          {isDerived && (
            <span className="rounded bg-concept-surface px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-concept-foreground">
              {t("derivedBadge")}
            </span>
          )}
        </div>
        <p className="mt-1 text-2xs text-foreground-subtle">
          {t("constraintCount", { count: rule.constraints?.length ?? 0 })}
        </p>
      </div>
      <div className="flex items-center gap-1.5">
        <Button variant="ghost" size="xs" onClick={onEdit} disabled={busy}>
          {isDerived ? t("viewButton") : t("editButton")}
        </Button>
        <Button
          variant="ghost"
          size="xs"
          onClick={onDelete}
          disabled={busy || isDerived}
        >
          {t("deleteButton")}
        </Button>
      </div>
    </div>
  );
}

function severityClass(severity: RuleDef["severity"]): string {
  switch (severity) {
    case "violation":
      return "bg-danger-surface text-danger-foreground dark:text-danger-foreground";
    case "warning":
      return "bg-warning-surface text-warning-foreground";
    case "info":
      return "bg-info-surface text-info-foreground";
    default:
      return "bg-surface-inset text-foreground-muted";
  }
}

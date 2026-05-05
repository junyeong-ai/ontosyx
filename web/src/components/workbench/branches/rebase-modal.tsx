"use client";

import { useTranslations } from "next-intl";

import { Modal } from "@/components/ui/modal";
import { Button } from "@/components/ui/button";
import { Heading } from "@/components/ui/heading";
import {
  useRebaseDraft,
  useRebasePreview,
} from "@/hooks/api/use-ontology-branches";
import { toast } from "@/components/ui/toast";
import type {
  ConflictAxis,
  PropertyConflictAtom,
  RebaseConflict,
} from "@/types/ontology-branches";

interface RebaseModalProps {
  draftId: string | null;
  draftTitle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * One ConflictAxis line — locale-aware copy plus the head/draft
 * value pair. Property-overlap expands its atomic axes inline so
 * the operator sees every clash without a second click.
 */
function ConflictAxisLine({ axis }: { axis: ConflictAxis }) {
  const t = useTranslations("workbench.branches.rebaseModal");
  switch (axis.axis) {
    case "label":
      return (
        <li>
          {t("axis.label", { head: axis.head, draft: axis.draft })}
        </li>
      );
    case "description":
      return (
        <li>
          {t("axis.description", {
            head: axis.head,
            draft: axis.draft,
          })}
        </li>
      );
    case "source":
      return (
        <li>
          {t("axis.source", { head: axis.head, draft: axis.draft })}
        </li>
      );
    case "target":
      return (
        <li>
          {t("axis.target", { head: axis.head, draft: axis.draft })}
        </li>
      );
    case "cardinality":
      return (
        <li>
          {t("axis.cardinality", {
            head: axis.head,
            draft: axis.draft,
          })}
        </li>
      );
    case "property_add_add":
      return (
        <li>
          {t("axis.propertyAddAdd", { name: axis.property_name })}
        </li>
      );
    case "property_modify_remove":
      return (
        <li>
          {axis.modifier === "draft"
            ? t("axis.propertyDraftModifyHeadRemove", {
                name: axis.property_name,
              })
            : t("axis.propertyHeadModifyDraftRemove", {
                name: axis.property_name,
              })}
        </li>
      );
    case "property_overlap":
      return (
        <li>
          <span>
            {t("axis.propertyOverlap", { name: axis.property_name })}
          </span>
          <ul className="ms-4 mt-1 list-disc space-y-0.5 text-foreground-muted">
            {axis.atoms.map((atom, idx) => (
              <PropertyAtomLine key={idx} atom={atom} />
            ))}
          </ul>
        </li>
      );
    default:
      return null;
  }
}

function PropertyAtomLine({ atom }: { atom: PropertyConflictAtom }) {
  const t = useTranslations("workbench.branches.rebaseModal");
  switch (atom.axis) {
    case "type":
      return (
        <li>
          {t("propertyAtom.type", {
            head: atom.head,
            draft: atom.draft,
          })}
        </li>
      );
    case "nullability":
      return (
        <li>
          {t("propertyAtom.nullability", {
            head: atom.head ? "nullable" : "required",
            draft: atom.draft ? "nullable" : "required",
          })}
        </li>
      );
    case "description":
      return (
        <li>
          {t("propertyAtom.description", {
            head: atom.head,
            draft: atom.draft,
          })}
        </li>
      );
    case "default_value":
      return (
        <li>
          {t("propertyAtom.defaultValue", {
            head: atom.head ?? "—",
            draft: atom.draft ?? "—",
          })}
        </li>
      );
    default:
      return null;
  }
}

function ConflictRow({ conflict }: { conflict: RebaseConflict }) {
  const t = useTranslations("workbench.branches.rebaseModal");
  const tBadge = (kind: "node" | "edge") =>
    kind === "node" ? t("badge.node") : t("badge.edge");
  switch (conflict.kind) {
    case "add_add":
      return (
        <li className="rounded-lg border border-divider bg-surface-base p-3 text-xs">
          <div className="flex items-baseline gap-2">
            <span className="rounded-full bg-warning-surface px-2 py-0.5 text-2xs font-medium text-warning-foreground">
              {t("conflict.addAdd")}
            </span>
            <span className="rounded-full bg-surface-inset px-2 py-0.5 text-2xs text-foreground-muted">
              {tBadge(conflict.entity_kind)}
            </span>
            <span className="font-medium">
              {conflict.label || conflict.entity_id}
            </span>
          </div>
          <p className="mt-1 text-foreground-muted">
            {t("conflict.addAddDescription")}
          </p>
        </li>
      );
    case "modify_remove":
      return (
        <li className="rounded-lg border border-divider bg-surface-base p-3 text-xs">
          <div className="flex items-baseline gap-2">
            <span className="rounded-full bg-danger-surface px-2 py-0.5 text-2xs font-medium text-danger-foreground">
              {conflict.modifier === "draft"
                ? t("conflict.draftModifyHeadRemove")
                : t("conflict.headModifyDraftRemove")}
            </span>
            <span className="rounded-full bg-surface-inset px-2 py-0.5 text-2xs text-foreground-muted">
              {tBadge(conflict.entity_kind)}
            </span>
            <span className="font-medium">
              {conflict.label || conflict.entity_id}
            </span>
          </div>
        </li>
      );
    case "modify_modify":
      return (
        <li className="rounded-lg border border-divider bg-surface-base p-3 text-xs">
          <div className="flex items-baseline gap-2">
            <span className="rounded-full bg-info-surface px-2 py-0.5 text-2xs font-medium text-info-foreground">
              {t("conflict.modifyModify")}
            </span>
            <span className="rounded-full bg-surface-inset px-2 py-0.5 text-2xs text-foreground-muted">
              {tBadge(conflict.entity_kind)}
            </span>
            <span className="font-medium">
              {conflict.label || conflict.entity_id}
            </span>
          </div>
          {conflict.axes.length > 0 && (
            <ul className="mt-2 ms-2 space-y-1 text-foreground-muted">
              {conflict.axes.map((axis, idx) => (
                <ConflictAxisLine key={idx} axis={axis} />
              ))}
            </ul>
          )}
        </li>
      );
    default:
      return null;
  }
}

export function RebaseModal({
  draftId,
  draftTitle,
  open,
  onOpenChange,
}: RebaseModalProps) {
  const t = useTranslations("workbench.branches.rebaseModal");
  const tCommon = useTranslations("common");
  const preview = useRebasePreview(open ? draftId : null);
  const rebase = useRebaseDraft();

  const conflicts = preview.data?.analysis.conflicts ?? [];
  const isClean = !preview.isLoading && !preview.isError && conflicts.length === 0;
  const alreadyAtHead = !!preview.data?.already_at_head;

  const onConfirm = (acknowledgeConflicts: boolean) => {
    if (!draftId) return;
    rebase.mutate(
      { draftId, acknowledgeConflicts },
      {
        onSuccess: () => {
          toast.success(t("toast.success"));
          onOpenChange(false);
        },
        onError: (err) => {
          toast.error(
            t("toast.error", {
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        },
      },
    );
  };

  const footer = preview.isLoading ? null : alreadyAtHead ? (
    <Button type="button" onClick={() => onOpenChange(false)}>
      {tCommon("close")}
    </Button>
  ) : isClean ? (
    <>
      <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
        {tCommon("cancel")}
      </Button>
      <Button
        type="button"
        onClick={() => onConfirm(false)}
        loading={rebase.isPending}
        disabled={rebase.isPending}
      >
        {t("confirmClean")}
      </Button>
    </>
  ) : (
    <>
      <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
        {tCommon("cancel")}
      </Button>
      <Button
        type="button"
        variant="danger"
        onClick={() => onConfirm(true)}
        loading={rebase.isPending}
        disabled={rebase.isPending}
      >
        {t("confirmAcknowledge")}
      </Button>
    </>
  );

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t("title", { title: draftTitle })}
      description={t("description")}
      size="xl"
      footer={footer}
    >
      {preview.isLoading ? (
        <p className="text-sm text-foreground-muted">{tCommon("loading")}</p>
      ) : preview.isError ? (
        <p className="text-sm text-danger-foreground">
          {tCommon("loadError.title")}
        </p>
      ) : alreadyAtHead ? (
        <p className="text-sm text-foreground-muted">{t("alreadyAtHead")}</p>
      ) : isClean ? (
        <p className="text-sm text-success-foreground">{t("clean")}</p>
      ) : (
        <div>
          <Heading level={3} size={6}>
            {t("conflictsHeading", { count: conflicts.length })}
          </Heading>
          <ul className="mt-2 space-y-2">
            {conflicts.map((c, idx) => (
              <ConflictRow key={idx} conflict={c} />
            ))}
          </ul>
        </div>
      )}
    </Modal>
  );
}

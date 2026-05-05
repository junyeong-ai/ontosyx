"use client";

import { useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { ExternalLink, Link, Unlink } from "lucide-react";
import { toast } from "@/components/ui/toast";

import { Tooltip } from "@/components/ui/tooltip";
import {
  useApplyBindingEdits,
  useSuggestTerms,
} from "@/hooks/api/use-binding-suggestions";
import type {
  BindingEditOp,
  OwnerKind,
  TermCandidate,
} from "@/lib/api/binding-suggestions";

export interface LinkTermDropdownProps {
  ontologyId: string;
  expectedVersion: number;
  ownerKind: OwnerKind;
  ownerTypeId: string;
  propertyId: string;
  /**
   * Currently-bound term id, rendered in the "linked" state. When
   * present, the button offers an unbind action instead of fetching
   * new suggestions.
   */
  boundTermId?: string | null;
}

const POLICY = { max_results: 3 } as const;

export function LinkTermDropdown(props: LinkTermDropdownProps) {
  const t = useTranslations("inspector.binding");
  const {
    ontologyId,
    expectedVersion,
    ownerKind,
    ownerTypeId,
    propertyId,
    boundTermId,
  } = props;
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const suggest = useSuggestTerms(ontologyId);
  const apply = useApplyBindingEdits(ontologyId);

  // Refetch suggestions every time the popover opens so the operator
  // sees the current candidate set (term list changes as they add /
  // rename glossary entries in another pane).
  useEffect(() => {
    if (!open || boundTermId) return;
    suggest.mutate({
      ownerKind,
      ownerTypeId,
      propertyId,
      policy: POLICY,
    });
    // `suggest` is a stable object reference from the mutation hook,
    // but including it as a dep triggers infinite re-fires because
    // TanStack replaces the mutation state on each call. The identity
    // axis we care about is the four ids + open flag.
  }, [open, boundTermId, ownerKind, ownerTypeId, propertyId, suggest.mutate]);

  // Click-outside closes the popover.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  const commitBinding = (glossaryTermId: string | null, label?: string) => {
    const op: BindingEditOp = glossaryTermId
      ? {
          op: "bind_property",
          owner: { kind: ownerKind, type_id: ownerTypeId },
          property_id: propertyId,
          binding: { kind: "glossary", id: glossaryTermId },
        }
      : {
          op: "unbind_property",
          owner: { kind: ownerKind, type_id: ownerTypeId },
          property_id: propertyId,
          // Unbind by `(kind, id)` selector. We always carry the
          // previously bound term id on `boundTermId`, so use that
          // to compose the exact handle the BE expects.
          target: { kind: "glossary", id: boundTermId ?? "" },
        };
    apply.mutate(
      {
        expected_version: expectedVersion,
        operations: [op],
        message: glossaryTermId
          ? `bind property ${propertyId} → glossary ${glossaryTermId}`
          : `unbind property ${propertyId} from glossary`,
      },
      {
        onSuccess: () => {
          setOpen(false);
          toast.success(
            glossaryTermId
              ? t("linkedToast", { term: label ?? glossaryTermId })
              : t("unlinkedToast"),
          );
        },
        onError: (err) => {
          toast.error(
            err instanceof Error ? err.message : t("applyFailed"),
          );
        },
      },
    );
  };

  return (
    <div ref={rootRef} className="relative">
      {boundTermId ? (
        <Tooltip content={t("unlinkTooltip", { term: boundTermId })}>
          <button
            type="button"
            onClick={() => commitBinding(null)}
            disabled={apply.isPending}
            aria-label={t("unlinkAria", { term: boundTermId })}
            className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs text-concept-foreground hover:bg-concept-surface disabled:opacity-50"
          >
            <ExternalLink className="h-2.5 w-2.5" />
            <span className="max-w-[100px] truncate">{boundTermId}</span>
          </button>
        </Tooltip>
      ) : (
        <Tooltip content={t("linkTooltip")}>
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-label={t("linkAria")}
            aria-expanded={open}
            className="rounded p-0.5 text-foreground-muted opacity-0 hover:bg-surface-inset hover:text-concept-foreground group-hover:opacity-100 group-focus-within:opacity-100"
          >
            <Link className="h-2.5 w-2.5" />
          </button>
        </Tooltip>
      )}

      {open && !boundTermId && (
        <div
          role="listbox"
          aria-label={t("suggestionsLabel")}
          className="absolute end-0 top-full z-popover mt-1 w-64 rounded-md border border-divider bg-surface-base p-2 shadow-3"
        >
          {suggest.isPending && (
            <p className="px-2 py-1.5 text-2xs text-foreground-muted">
              {t("loading")}
            </p>
          )}
          {suggest.isError && (
            <p className="px-2 py-1.5 text-2xs text-danger-foreground">
              {t("fetchFailed")}
            </p>
          )}
          {suggest.isSuccess && suggest.data.candidates.length === 0 && (
            <p className="px-2 py-1.5 text-2xs text-foreground-muted">
              {t("noSuggestions")}
            </p>
          )}
          {suggest.isSuccess &&
            suggest.data.candidates.map((c) => (
              <CandidateRow
                key={c.term_id}
                candidate={c}
                onPick={() => commitBinding(c.term_id, c.term)}
                disabled={apply.isPending}
              />
            ))}
          {apply.isPending && (
            <p className="px-2 pt-1.5 text-2xs italic text-foreground-muted">
              {t("applying")}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

function CandidateRow({
  candidate,
  onPick,
  disabled,
}: {
  candidate: TermCandidate;
  onPick: () => void;
  disabled: boolean;
}) {
  const pct = Math.round(Math.max(0, Math.min(1, candidate.score)) * 100);
  return (
    <button
      role="option"
      aria-selected="false"
      type="button"
      onClick={onPick}
      disabled={disabled}
      className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-start hover:bg-concept-surface disabled:opacity-50"
    >
      <Unlink className="h-2.5 w-2.5 text-foreground-muted" />
      <span className="min-w-0 flex-1 truncate text-2xs font-medium text-foreground-strong">
        {candidate.term}
      </span>
      <span className="shrink-0 rounded bg-concept-surface px-1 text-2xs font-medium text-concept-foreground">
        {pct}%
      </span>
    </button>
  );
}

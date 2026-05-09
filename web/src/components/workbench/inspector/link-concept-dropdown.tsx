"use client";

import { useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { ExternalLink, Link, Unlink } from "lucide-react";
import { toast } from "@/components/ui/toast";

import { Tooltip } from "@/components/ui/tooltip";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import {
  useApplyBindingEdits,
  useSuggestConcepts,
} from "@/hooks/api/use-binding-suggestions";
import type {
  BindingEditOp,
  OwnerKind,
  ConceptCandidate,
} from "@/lib/api/binding-suggestions";
import type { PropertyBindingHandle } from "@/types/ontology";

export interface LinkConceptDropdownProps {
  ontologyId: string;
  expectedVersion: number;
  ownerKind: OwnerKind;
  ownerTypeId: string;
  propertyId: string;
  /**
   * Currently-bound semantic target, rendered in the "linked" state. When
   * present, the button offers an unbind action instead of fetching
   * new suggestions.
   */
  boundBinding?: PropertyBindingHandle | null;
}

const POLICY = { max_results: 3 } as const;

export function LinkConceptDropdown(props: LinkConceptDropdownProps) {
  const t = useTranslations("inspector.binding");
  const {
    ontologyId,
    expectedVersion,
    ownerKind,
    ownerTypeId,
    propertyId,
    boundBinding,
  } = props;
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const suggest = useSuggestConcepts(ontologyId);
  const apply = useApplyBindingEdits(ontologyId);
  const localeChain = useLocaleChain();

  // Refetch suggestions every time the popover opens so the operator
  // sees the current candidate set (concept labels change as they
  // add or rename glossary lexicalizations in another pane).
  useEffect(() => {
    if (!open || boundBinding) return;
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
  }, [open, boundBinding, ownerKind, ownerTypeId, propertyId, suggest.mutate]);

  // Click-outside closes the popover.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  const commitBinding = (candidate: ConceptCandidate | null) => {
    const op: BindingEditOp = candidate
      ? {
          op: "bind_property",
          owner: { kind: ownerKind, type_id: ownerTypeId },
          property_id: propertyId,
          binding: { kind: "concept", id: candidate.concept_id },
        }
      : {
          op: "unbind_property",
          owner: { kind: ownerKind, type_id: ownerTypeId },
          property_id: propertyId,
          target: boundBinding ?? { kind: "concept", id: "" },
        };
    apply.mutate(
      {
        expected_version: expectedVersion,
        operations: [op],
        message: candidate
          ? `bind property ${propertyId} → concept ${candidate.concept_id}`
          : `unbind property ${propertyId} from concept`,
      },
      {
        onSuccess: () => {
          setOpen(false);
          toast.success(
            candidate
              ? t("linkedToast", {
                  term: localize(candidate.term, localeChain) || candidate.term_id,
                })
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

  const bindableCandidates = suggest.data?.candidates ?? [];
  const boundLabel = boundBinding?.id ?? "";

  return (
    <div ref={rootRef} className="relative">
      {boundBinding ? (
        <Tooltip content={t("unlinkTooltip", { term: boundLabel })}>
          <button
            type="button"
            onClick={() => commitBinding(null)}
            disabled={apply.isPending}
            aria-label={t("unlinkAria", { term: boundLabel })}
            className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs text-concept-foreground hover:bg-concept-surface disabled:opacity-50"
          >
            <ExternalLink className="h-2.5 w-2.5" />
            <span className="max-w-[100px] truncate">{boundLabel}</span>
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

      {open && !boundBinding && (
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
          {suggest.isSuccess && bindableCandidates.length === 0 && (
            <p className="px-2 py-1.5 text-2xs text-foreground-muted">
              {t("noSuggestions")}
            </p>
          )}
          {suggest.isSuccess &&
            bindableCandidates.map((c) => (
              <CandidateRow
                key={c.term_id}
                candidate={c}
                localeChain={localeChain}
                onPick={() => commitBinding(c)}
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
  localeChain,
  onPick,
  disabled,
}: {
  candidate: ConceptCandidate;
  localeChain: readonly string[];
  onPick: () => void;
  disabled: boolean;
}) {
  const pct = Math.round(Math.max(0, Math.min(1, candidate.score)) * 100);
  const termLabel = localize(candidate.term, localeChain) || candidate.term_id;
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
        {termLabel}
      </span>
      <span className="shrink-0 rounded bg-concept-surface px-1 text-2xs font-medium text-concept-foreground">
        {pct}%
      </span>
    </button>
  );
}

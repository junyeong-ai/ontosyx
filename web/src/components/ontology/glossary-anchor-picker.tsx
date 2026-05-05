"use client";

import { useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PlusSignIcon,
  Cancel01Icon,
  AlertCircleIcon,
  Search01Icon,
} from "@hugeicons/core-free-icons";

import type { GlossaryTermDef } from "@/lib/api/edit-ops";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import { SearchInput } from "@/components/ui/form-input";

export interface GlossaryAnchorPickerProps {
  /** Currently-selected term ids. */
  value: readonly string[];
  /** All glossary terms available in this ontology. */
  glossary: readonly GlossaryTermDef[];
  /** Fired with the new id list whenever the operator adds or removes an
   *  anchor. The picker stays controlled — callers drive persistence. */
  onChange: (next: string[]) => void;
  /** When true, hide every add/remove affordance and render the
   *  selection as a static badge list. */
  readOnly?: boolean;
}

/**
 * Multi-select picker for `glossary_anchors` on a NodeType / EdgeType.
 *
 * Distinct from the inspector's `LinkTermDropdown` which is a
 * single-select, AI-suggested binding for one PROPERTY. Anchors live
 * at the type level — a Customer node can anchor to multiple business
 * concepts (e.g., `gt-customer` AND `gt-loyalty-tier`). The picker
 * exposes the full glossary catalogue with synchronous filter; AI
 * suggestion lives elsewhere (suggest-anchors API) and feeds back
 * through the same `onChange` surface.
 */
export function GlossaryAnchorPicker({
  value,
  glossary,
  onChange,
  readOnly = false,
}: GlossaryAnchorPickerProps) {
  const t = useTranslations("ontology.glossaryAnchorPicker");
  const localeChain = useLocaleChain();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const rootRef = useRef<HTMLDivElement | null>(null);

  const termById = useMemo(() => {
    const map = new Map<string, GlossaryTermDef>();
    for (const term of glossary) map.set(term.id, term);
    return map;
  }, [glossary]);

  const candidates = useMemo(() => {
    const selected = new Set(value);
    const needle = query.trim().toLowerCase();
    return glossary
      .filter((term) => !selected.has(term.id))
      .filter((term) => {
        if (!needle) return true;
        const label = localizePresent(term.term, localeChain) ?? "";
        const display = term.display_name
          ? (localizePresent(term.display_name, localeChain) ?? "")
          : "";
        return (
          term.id.toLowerCase().includes(needle) ||
          label.toLowerCase().includes(needle) ||
          display.toLowerCase().includes(needle)
        );
      })
      .slice(0, 10);
  }, [glossary, value, query, localeChain]);

  const handleAdd = (id: string) => {
    onChange([...value, id]);
    setQuery("");
    setOpen(false);
  };

  const handleRemove = (id: string) => {
    onChange(value.filter((v) => v !== id));
  };

  return (
    <div ref={rootRef} className="space-y-2">
      {value.length === 0 && readOnly && (
        <p className="text-2xs italic text-foreground-muted">
          {t("emptyReadOnly")}
        </p>
      )}

      {value.length > 0 && (
        <ul className="flex flex-wrap gap-1.5">
          {value.map((id) => {
            const term = termById.get(id);
            return (
              <AnchorChip
                key={id}
                id={id}
                label={
                  term
                    ? (localizePresent(
                        term.display_name ?? term.term,
                        localeChain,
                      ) ?? id)
                    : id
                }
                missing={!term}
                onRemove={readOnly ? undefined : () => handleRemove(id)}
                missingLabel={t("missingTooltip", { id })}
                removeLabel={t("removeAria", { id })}
              />
            );
          })}
        </ul>
      )}

      {!readOnly && (
        <div className="relative">
          {open ? (
            <SearchPopover
              query={query}
              onQueryChange={setQuery}
              candidates={candidates}
              localeChain={localeChain}
              onPick={handleAdd}
              onClose={() => {
                setOpen(false);
                setQuery("");
              }}
              labelEmpty={t("noMatches")}
              labelSearch={t("searchPlaceholder")}
            />
          ) : (
            <button
              type="button"
              onClick={() => setOpen(true)}
              className="inline-flex items-center gap-1 rounded border border-dashed border-divider px-2 py-1 text-2xs text-foreground-muted hover:border-concept-border hover:text-concept-foreground"
            >
              <HugeiconsIcon
                icon={PlusSignIcon}
                className="h-3 w-3"
                size="100%"
              />
              {t("addAction")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function AnchorChip({
  id,
  label,
  missing,
  onRemove,
  missingLabel,
  removeLabel,
}: {
  id: string;
  label: string;
  missing: boolean;
  onRemove?: () => void;
  missingLabel: string;
  removeLabel: string;
}) {
  const baseColor = missing
    ? "bg-danger-surface text-danger-foreground"
    : "bg-concept-surface text-concept-foreground";
  return (
    <li
      className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-2xs ${baseColor}`}
    >
      {missing && (
        <span title={missingLabel}>
          <HugeiconsIcon
            icon={AlertCircleIcon}
            className="h-2.5 w-2.5"
            size="100%"
          />
        </span>
      )}
      <span className="font-medium">{label}</span>
      <span className="font-mono text-2xs opacity-60">{id}</span>
      {onRemove && (
        <button
          type="button"
          onClick={onRemove}
          aria-label={removeLabel}
          className="rounded-full p-0.5 hover:bg-concept-surface"
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-2.5 w-2.5" size="100%" />
        </button>
      )}
    </li>
  );
}

function SearchPopover({
  query,
  onQueryChange,
  candidates,
  localeChain,
  onPick,
  onClose,
  labelEmpty,
  labelSearch,
}: {
  query: string;
  onQueryChange: (next: string) => void;
  candidates: readonly GlossaryTermDef[];
  localeChain: readonly string[];
  onPick: (id: string) => void;
  onClose: () => void;
  labelEmpty: string;
  labelSearch: string;
}) {
  return (
    <div className="rounded-md border border-divider bg-surface-base p-2 shadow-1">
      <div className="border-b border-divider-soft pb-1.5">
        <SearchInput
          autoFocus
          type="text"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
            if (e.key === "Enter" && candidates[0]) onPick(candidates[0].id);
          }}
          placeholder={labelSearch}
          aria-label={labelSearch}
          density="compact"
          leadingIcon={Search01Icon}
        />
      </div>
      <ul className="mt-1 max-h-60 space-y-0.5 overflow-y-auto">
        {candidates.length === 0 ? (
          <li className="px-1.5 py-1 text-2xs italic text-foreground-muted">
            {labelEmpty}
          </li>
        ) : (
          candidates.map((term) => {
            const label =
              localizePresent(term.term, localeChain) ?? term.id;
            const description = term.description
              ? localizePresent(term.description, localeChain)
              : null;
            return (
              <li key={term.id}>
                <button
                  type="button"
                  onClick={() => onPick(term.id)}
                  className="flex w-full flex-col rounded px-1.5 py-1 text-start hover:bg-concept-surface"
                >
                  <span className="flex items-baseline gap-2 text-2xs">
                    <span className="font-medium text-foreground-strong">
                      {label}
                    </span>
                    <span className="font-mono text-2xs text-foreground-muted">
                      {term.id}
                    </span>
                  </span>
                  {description && (
                    <span className="text-2xs text-foreground-muted line-clamp-1">
                      {description}
                    </span>
                  )}
                </button>
              </li>
            );
          })
        )}
      </ul>
    </div>
  );
}

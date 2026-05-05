"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlusSignIcon, Search01Icon } from "@hugeicons/core-free-icons";

import { SearchInput } from "@/components/ui/form-input";

import type { GlossaryTermDef } from "@/lib/api/edit-ops";
import { localize, localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import { compareKorean } from "@/lib/locale/sort";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// TermTree — left pane of the Glossary workbench.
//
// Single search-filtered list grouped by category with a deprecated
// section pinned at the bottom. The "tree" name is aspirational —
// `GlossaryTermDef` carries SKOS broader/narrower in `related_terms`,
// but rendering a true broader-tree adds ambiguity (cycles, multiple
// parents) without value at the volumes we expect. Group-by-category
// is the cheap-and-explicit alternative; if a taxonomy emerges we can
// promote it later.
// ---------------------------------------------------------------------------

export interface TermAnchorCounts {
  // Pre-computed per-term anchor counts so the tree renders without
  // walking the ontology on every selection click.
  byTermId: Map<string, number>;
}

interface TermTreeProps {
  terms: readonly GlossaryTermDef[];
  selectedTermId: string | null;
  onSelect: (termId: string) => void;
  onCreate: () => void;
  anchorCounts: TermAnchorCounts;
}

const UNCATEGORISED = "__uncategorised__";

export function TermTree({
  terms,
  selectedTermId,
  onSelect,
  onCreate,
  anchorCounts,
}: TermTreeProps) {
  const t = useTranslations("workbench.glossary.tree");
  const localeChain = useLocaleChain();
  const [search, setSearch] = useState("");

  const filteredAndGrouped = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const matches = (def: GlossaryTermDef) => {
      if (needle.length === 0) return true;
      const haystack = [
        localize(def.term, localeChain),
        def.display_name ? localizePresent(def.display_name, localeChain) : "",
        ...(def.aliases ?? []).map((a) => localize(a, localeChain)),
        def.category ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(needle);
    };

    const active: GlossaryTermDef[] = [];
    const deprecated: GlossaryTermDef[] = [];
    for (const term of terms) {
      if (!matches(term)) continue;
      const state = term.lifecycle?.state ?? "active";
      if (state === "active") active.push(term);
      else deprecated.push(term);
    }

    const byCategory = new Map<string, GlossaryTermDef[]>();
    for (const term of active) {
      const key = term.category && term.category.trim().length > 0
        ? term.category
        : UNCATEGORISED;
      const bucket = byCategory.get(key) ?? [];
      bucket.push(term);
      byCategory.set(key, bucket);
    }
    for (const bucket of byCategory.values()) {
      bucket.sort((a, b) =>
        compareKorean(localize(a.term, localeChain), localize(b.term, localeChain)),
      );
    }
    deprecated.sort((a, b) =>
      compareKorean(localize(a.term, localeChain), localize(b.term, localeChain)),
    );
    const categories = Array.from(byCategory.keys()).sort((a, b) => {
      // Pin uncategorised to the end so categorised concepts surface first.
      if (a === UNCATEGORISED) return 1;
      if (b === UNCATEGORISED) return -1;
      return a.localeCompare(b);
    });

    return { categories, byCategory, deprecated, totalMatches: active.length + deprecated.length };
  }, [terms, search, localeChain]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-divider px-3 py-2">
        <h2 className="flex-1 text-xs font-semibold text-foreground-strong">
          {t("heading", { count: terms.length })}
        </h2>
        <button
          type="button"
          onClick={onCreate}
          className="inline-flex items-center gap-1 rounded bg-brand-solid px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-onbrand shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid"
          aria-label={t("createAria")}
        >
          <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
          {t("createLabel")}
        </button>
      </div>

      <div className="border-b border-divider px-3 py-2">
        <SearchInput
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t("searchPlaceholder")}
          aria-label={t("searchAria")}
          density="compact"
          leadingIcon={Search01Icon}
        />
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-2">
        {filteredAndGrouped.totalMatches === 0 && (
          <p className="px-2 py-3 text-2xs italic text-foreground-muted">
            {search.trim().length > 0
              ? t("emptySearch", { query: search })
              : t("emptyAll")}
          </p>
        )}
        {filteredAndGrouped.categories.map((category) => (
          <CategoryGroup
            key={category}
            label={
              category === UNCATEGORISED ? t("uncategorised") : category
            }
            terms={filteredAndGrouped.byCategory.get(category) ?? []}
            selectedTermId={selectedTermId}
            onSelect={onSelect}
            anchorCounts={anchorCounts}
          />
        ))}
        {filteredAndGrouped.deprecated.length > 0 && (
          <CategoryGroup
            label={t("deprecatedGroup")}
            terms={filteredAndGrouped.deprecated}
            selectedTermId={selectedTermId}
            onSelect={onSelect}
            anchorCounts={anchorCounts}
            tone="muted"
          />
        )}
      </div>
    </div>
  );
}

function CategoryGroup({
  label,
  terms,
  selectedTermId,
  onSelect,
  anchorCounts,
  tone = "default",
}: {
  label: string;
  terms: readonly GlossaryTermDef[];
  selectedTermId: string | null;
  onSelect: (termId: string) => void;
  anchorCounts: TermAnchorCounts;
  tone?: "default" | "muted";
}) {
  const localeChain = useLocaleChain();
  return (
    <div className="mt-2 flex flex-col gap-0.5">
      <span
        className={cn(
          "px-2 pb-0.5 text-2xs font-semibold uppercase tracking-wider",
          tone === "muted"
            ? "text-foreground-subtle"
            : "text-foreground-muted",
        )}
      >
        {label}
      </span>
      {terms.map((term) => {
        const isSelected = selectedTermId === term.id;
        const label = localize(term.term, localeChain);
        const usageCount = anchorCounts.byTermId.get(term.id) ?? 0;
        const isInactive = (term.lifecycle?.state ?? "active") !== "active";
        return (
          <button
            key={term.id}
            type="button"
            onClick={() => onSelect(term.id)}
            className={cn(
              "group flex items-center gap-2 rounded px-2 py-1 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              isSelected
                ? "bg-brand-surface-strong text-brand-foreground-strong"
                : "text-foreground hover:bg-surface-inset",
            )}
          >
            <span
              className={cn(
                "flex-1 truncate text-2xs font-medium",
                isInactive && "line-through opacity-70",
              )}
            >
              {label}
            </span>
            {usageCount > 0 && (
              <span
                className={cn(
                  "rounded px-1.5 py-0 text-2xs font-medium",
                  isSelected
                    ? "bg-brand-surface-strong text-brand-foreground-strong"
                    : "bg-surface-inset text-foreground-muted",
                )}
                title={`${usageCount}`}
              >
                {usageCount}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

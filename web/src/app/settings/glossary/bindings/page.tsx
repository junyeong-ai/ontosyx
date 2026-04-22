"use client";

// Phase 4.5 reverse — /settings/glossary/bindings host page.
//
// Picks the operator's current ontology from `useOntologies` (first
// page, most-recent version), threads its id + version into the
// panel. Once a proper Glossary CRUD admin page lands, the panel
// will embed there alongside the term list; until then this
// standalone page gives designers a way to batch-bind without
// editing each property one-by-one in the inspector.

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { useOntologies } from "@/hooks/api/use-ontologies";
import { Spinner } from "@/components/ui/spinner";
import { GlossaryBindingPanel } from "@/components/settings/glossary/binding-panel";

export default function GlossaryBindingsPage() {
  const t = useTranslations("settings.glossaryBinding");
  const ontologies = useOntologies({ limit: 1 });

  const current = useMemo(
    () => ontologies.data?.items?.[0],
    [ontologies.data],
  );
  const currentVersion = current?.current_version?.version;

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

      {ontologies.isLoading && (
        <div className="flex items-center justify-center py-10">
          <Spinner />
        </div>
      )}

      {!ontologies.isLoading && !current && (
        <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
          {t("noOntology")}
        </p>
      )}

      {current && currentVersion !== undefined && (
        <GlossaryBindingPanel
          ontologyId={current.id}
          expectedVersion={Number(currentVersion)}
        />
      )}
    </div>
  );
}

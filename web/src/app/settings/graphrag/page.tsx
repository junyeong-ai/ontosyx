"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";

import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { Heading } from "@/components/ui/heading";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Spinner } from "@/components/ui/spinner";
import { FormInput } from "@/components/ui/form-input";
import {
  useCommunitySummaries,
  useSearchCommunitySummaries,
} from "@/hooks/api/use-community-summaries";
import type { CommunitySummary } from "@/lib/api/community-summaries";

export default function GraphRagSettingsPage() {
  const t = useTranslations("settings.graphrag");
  const [query, setQuery] = useState("");
  const trimmed = query.trim();

  const allQuery = useCommunitySummaries({ enabled: trimmed.length === 0 });
  const searchQuery = useSearchCommunitySummaries(
    { q: trimmed, topK: 50 },
    { enabled: trimmed.length > 0 },
  );

  const { data, isLoading, isError } =
    trimmed.length > 0 ? searchQuery : allQuery;

  const items: CommunitySummary[] = useMemo(
    () => data?.items ?? [],
    [data],
  );

  return (
    <SettingsPageShell title={t("title")} subtitle={t("subtitle")}>
      <div className="flex flex-col gap-4">
        <FormInput
          aria-label={t("search.ariaLabel")}
          placeholder={t("search.placeholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {isLoading ? (
          <div className="flex h-48 items-center justify-center">
            <Spinner />
          </div>
        ) : isError ? (
          <ErrorState title={t("list.loadFailed")} />
        ) : items.length === 0 ? (
          <EmptyState
            kind={trimmed.length > 0 ? "no-results" : "pending"}
            title={
              trimmed.length > 0 ? t("list.searchEmpty") : t("list.empty")
            }
          />
        ) : (
          <ul className="flex flex-col gap-3" role="list">
            {items.map((c) => (
              <CommunityRow key={c.id} community={c} />
            ))}
          </ul>
        )}
      </div>
    </SettingsPageShell>
  );
}

function CommunityRow({ community }: { community: CommunitySummary }) {
  const t = useTranslations("settings.graphrag");
  const formattedTs = useMemo(() => {
    try {
      return new Date(community.generated_at).toLocaleString();
    } catch {
      return community.generated_at;
    }
  }, [community.generated_at]);
  return (
    <li className="rounded-lg border border-divider bg-surface-raised p-4">
      <div className="flex items-start justify-between gap-3">
        <Heading level={3} size={5}>
          {community.title}
        </Heading>
        <div className="flex shrink-0 items-center gap-2 text-2xs text-foreground-muted">
          <span className="rounded-full bg-brand-surface px-2 py-0.5 text-brand-foreground">
            {t("list.level", { n: community.level })}
          </span>
          <span>
            {t("list.members", { n: community.member_logical_ids.length })}
          </span>
          <span>{t("list.generatedAt", { ts: formattedTs })}</span>
        </div>
      </div>
      <p className="mt-2 whitespace-pre-line text-sm text-foreground">
        {community.summary}
      </p>
    </li>
  );
}

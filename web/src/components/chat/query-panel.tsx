"use client";

import { useState, useRef } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlayIcon, CommandLineIcon } from "@hugeicons/core-free-icons";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Alert } from "@/components/ui/alert";
import { EmptyState } from "@/components/ui/empty-state";
import { WidgetToolbar } from "@/components/dashboard/widgets/widget-toolbar";
import { ResponseBasis } from "@/components/dashboard/widgets/response-basis";
import { rawQuery } from "@/lib/api";
import { cn } from "@/lib/cn";
import type { QueryResult } from "@/types/api";

export function QueryPanel() {
  const t = useTranslations("workbench.chat.query");
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleExecute = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const res = await rawQuery({ query: query.trim() });
      setResult(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Cmd/Ctrl + Enter to execute
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleExecute();
    }
  };

  const handleInput = () => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 300) + "px";
  };

  return (
    <div className="flex h-full flex-col">
      {/* Editor */}
      <div className="border-b border-divider bg-surface-base p-4">
        <div className="mx-auto max-w-4xl">
          <div className="flex items-center justify-between pb-3">
            <div className="flex items-center gap-2">
              <HugeiconsIcon icon={CommandLineIcon} className="h-4 w-4 text-muted-foreground" size="100%" />
              <h2 className="text-sm font-semibold">{t("title")}</h2>
            </div>
            <Button
              size="sm"
              onClick={handleExecute}
              disabled={loading || !query.trim()}
              className="gap-1.5"
            >
              {loading ? (
                <Spinner size="xs" />
              ) : (
                <HugeiconsIcon icon={PlayIcon} className="h-3.5 w-3.5" size="100%" />
              )}
              {loading ? t("runningLabel") : t("execute")}
            </Button>
          </div>
          <textarea
            ref={textareaRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            onInput={handleInput}
            placeholder={t("placeholder")}
            rows={4}
            className={cn(
              "w-full resize-none rounded-lg border border-divider bg-surface-raised p-3",
              "font-mono text-sm text-brand-foreground placeholder:text-foreground",
              "focus:border-brand-foreground focus:outline-none focus:ring-2 focus:ring-brand-foreground/50",
              "dark:border-divider",
            )}
          />
          <p className="mt-1.5 text-2xs text-muted-foreground">
            {typeof navigator !== "undefined" &&
            navigator.userAgent?.includes("Mac")
              ? t("execHintPrefixMac")
              : t("execHintPrefixOther")}
            {t("execHintSuffix")}
          </p>
        </div>
      </div>

      {/* Results */}
      <div className="flex-1 overflow-auto bg-surface-raised p-4">
        <div className="mx-auto max-w-4xl">
          {error && (
            <Alert variant="error" title={t("failedTitle")} onDismiss={() => setError(null)}>
              {error}
            </Alert>
          )}

          {result && result.rows?.length > 0 && (
            <div className="space-y-3">
              <WidgetToolbar spec={{ widget: "table" }} data={result} />
              <ResponseBasis provenance={result.metadata?.provenance} warnings={result.metadata?.warnings} />
            </div>
          )}
          {result && result.rows?.length === 0 && (
            <div className="space-y-3">
              <p className="py-4 text-center text-sm text-muted-foreground">
                {t("noRows")}
              </p>
              <ResponseBasis provenance={result.metadata?.provenance} warnings={result.metadata?.warnings} />
            </div>
          )}

          {!result && !error && (
            <EmptyState icon={CommandLineIcon} title={t("emptyPrompt")} />
          )}
        </div>
      </div>
    </div>
  );
}

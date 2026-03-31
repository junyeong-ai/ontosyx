"use client";

import { useState, useRef } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlayIcon, CommandLineIcon } from "@hugeicons/core-free-icons";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Alert } from "@/components/ui/alert";
import { WidgetWithToolbar } from "@/components/widgets/widget-toolbar";
import { rawQuery } from "@/lib/api";
import { cn } from "@/lib/cn";
import type { QueryResult } from "@/types/api";

export function QueryPanel() {
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
      <div className="border-b border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-950">
        <div className="mx-auto max-w-4xl">
          <div className="flex items-center justify-between pb-3">
            <div className="flex items-center gap-2">
              <HugeiconsIcon icon={CommandLineIcon} className="h-4 w-4 text-zinc-500" size="100%" />
              <h2 className="text-sm font-semibold">Raw Query</h2>
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
              {loading ? "Running..." : "Execute"}
            </Button>
          </div>
          <textarea
            ref={textareaRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            onInput={handleInput}
            placeholder="MATCH (n) RETURN n LIMIT 10"
            rows={4}
            className={cn(
              "w-full resize-none rounded-lg border border-zinc-200 bg-zinc-900 p-3",
              "font-mono text-sm text-emerald-400 placeholder:text-zinc-600",
              "focus:border-emerald-500 focus:outline-none focus:ring-2 focus:ring-emerald-500/50",
              "dark:border-zinc-700",
            )}
          />
          <p className="mt-1.5 text-[10px] text-zinc-400">
            {typeof navigator !== "undefined" &&
            navigator.userAgent?.includes("Mac")
              ? "\u2318"
              : "Ctrl"}
            +Enter to execute
          </p>
        </div>
      </div>

      {/* Results */}
      <div className="flex-1 overflow-auto bg-zinc-50/50 p-4 dark:bg-zinc-950">
        <div className="mx-auto max-w-4xl">
          {error && (
            <Alert variant="error" title="Query failed" onDismiss={() => setError(null)}>
              {error}
            </Alert>
          )}

          {result && result.rows?.length > 0 && (
            <WidgetWithToolbar
              spec={{ widget: "table" }}
              data={result}
            />
          )}
          {result && result.rows?.length === 0 && (
            <p className="py-4 text-center text-sm text-zinc-400">
              Query executed successfully — 0 rows returned
            </p>
          )}

          {!result && !error && (
            <div className="flex h-64 flex-col items-center justify-center text-center">
              <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-zinc-100 dark:bg-zinc-800">
                <HugeiconsIcon icon={CommandLineIcon} className="h-6 w-6 text-zinc-400 dark:text-zinc-500" size="100%" />
              </div>
              <p className="text-sm text-zinc-500">
                Write a Cypher query and hit Execute
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

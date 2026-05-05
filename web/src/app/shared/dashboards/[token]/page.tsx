"use client";

/**
 * Public shared dashboard. Resolves a share token via the proxy
 * route and handles three terminal states: 200 (render read-only),
 * 410 Gone (token expired/revoked → `<Expired />`), or any other
 * error (generic "not available"). Unauthenticated route; lives
 * outside the workspace shell.
 */

import { useEffect, useState } from "react";
import { use } from "react";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { Heading } from "@/components/ui/heading";
import Expired from "./expired";

interface SharedDashboardPayload {
  dashboard: {
    id: string;
    name: string;
    description: string | null;
    shared_at: string | null;
  };
  widgets: Array<{
    id: string;
    title: string;
    widget_type: string;
    last_result: unknown;
  }>;
}

type LoadState =
  | { kind: "loading" }
  | { kind: "ok"; payload: SharedDashboardPayload }
  | { kind: "expired" }
  | { kind: "not_found" }
  | { kind: "error"; message: string };

export default function SharedDashboardPage({
  params,
}: {
  params: Promise<{ token: string }>;
}) {
  const t = useTranslations("page.sharedDashboard");
  const { token } = use(params);
  const [state, setState] = useState<LoadState>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(
          `/api/proxy/shared/dashboards/${encodeURIComponent(token)}`,
          { cache: "no-store" },
        );
        if (cancelled) return;
        if (res.status === 410) {
          setState({ kind: "expired" });
          return;
        }
        if (res.status === 404) {
          setState({ kind: "not_found" });
          return;
        }
        if (!res.ok) {
          setState({ kind: "error", message: `HTTP ${res.status}` });
          return;
        }
        const payload = (await res.json()) as SharedDashboardPayload;
        setState({ kind: "ok", payload });
      } catch (err) {
        if (cancelled) return;
        setState({
          kind: "error",
          message: err instanceof Error ? err.message : "Network error",
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token]);

  if (state.kind === "loading") {
    return (
      <main id="main" className="flex min-h-dvh items-center justify-center bg-surface-raised">
        <div className="flex flex-col items-center gap-2">
          <Spinner size="md" className="text-brand-foreground" />
          <p className="text-xs text-foreground-muted">
            {t("loadingMessage")}
          </p>
        </div>
      </main>
    );
  }

  if (state.kind === "expired") {
    return <Expired />;
  }

  if (state.kind === "not_found") {
    return (
      <SharedTerminalState
        title={t("notFoundTitle")}
        subtitle={t("notFoundSubtitle")}
        body={t("notFoundBody")}
      />
    );
  }

  if (state.kind === "error") {
    return (
      <SharedTerminalState
        title={t("errorTitle")}
        subtitle={t("errorSubtitle")}
        body={t("errorBody", { message: state.message })}
      />
    );
  }

  // The workbench sets `body { overflow: hidden }` globally — allow
  // scroll here so long dashboards aren't clipped.
  return (
    <main id="main" className="h-dvh overflow-auto bg-surface-raised">
      <div className="mx-auto max-w-6xl px-6 py-8">
        <header className="border-b border-divider pb-4">
          <h1 className="text-xl font-semibold text-foreground-strong">
            {state.payload.dashboard.name}
          </h1>
          {state.payload.dashboard.description && (
            <p className="mt-1 text-sm text-foreground">
              {state.payload.dashboard.description}
            </p>
          )}
          <p className="mt-2 text-2xs text-foreground-muted">
            {t("header")}
          </p>
        </header>

        <div className="mt-6 space-y-3">
          {state.payload.widgets.length === 0 ? (
            <p className="text-sm text-foreground-muted">{t("noWidgets")}</p>
          ) : (
            state.payload.widgets.map((w) => (
              <div
                key={w.id}
                className="rounded-lg border border-divider bg-surface-base p-4"
              >
                <div className="flex items-center justify-between">
                  <h2 className="truncate text-sm font-medium text-foreground-strong">
                    {w.title}
                  </h2>
                  <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs text-foreground-muted">
                    {w.widget_type}
                  </span>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </main>
  );
}

function SharedTerminalState({
  title,
  subtitle,
  body,
}: {
  title: string;
  subtitle: string;
  body: string;
}) {
  return (
    <main id="main" className="flex min-h-dvh items-center justify-center bg-surface-raised px-4">
      <div className="w-full max-w-md rounded-xl border border-divider bg-surface-base p-6 text-center shadow-1">
        <Heading level={1} size={4}>
          {title}
        </Heading>
        <p className="mt-1 text-xs text-foreground-muted">{subtitle}</p>
        <p className="mt-4 text-sm text-foreground">{body}</p>
      </div>
    </main>
  );
}

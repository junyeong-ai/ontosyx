"use client";

/**
 * Public shared dashboard page.
 *
 * Resolves a share token → dashboard payload via the proxy route. Handles
 * three terminal states:
 *   - 200: render dashboard (read-only).
 *   - 410 Gone (Phase 4.10): the token has expired → show `<Expired />`.
 *   - any other error: show a generic "not available" screen.
 *
 * This is an unauthenticated route; no sidebar/header shell. The `/shared`
 * segment deliberately sits outside the root app layout's workspace UI.
 */

import { useEffect, useState } from "react";
import { use } from "react";
import { Spinner } from "@/components/ui/spinner";
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
      <div className="flex min-h-dvh items-center justify-center bg-zinc-50 dark:bg-zinc-950">
        <div className="flex flex-col items-center gap-2">
          <Spinner size="md" className="text-emerald-500" />
          <p className="text-xs text-zinc-400">
            공유된 대시보드를 불러오는 중…
          </p>
        </div>
      </div>
    );
  }

  if (state.kind === "expired") {
    return <Expired />;
  }

  if (state.kind === "not_found") {
    return (
      <SharedTerminalState
        title="존재하지 않는 공유 링크예요"
        subtitle="This share link does not exist."
        body="링크가 잘못 입력되었거나, 이미 해제된 링크일 수 있습니다."
      />
    );
  }

  if (state.kind === "error") {
    return (
      <SharedTerminalState
        title="대시보드를 불러올 수 없습니다"
        subtitle="Could not load the shared dashboard."
        body={`오류: ${state.message}. 잠시 후 다시 시도해 주세요.`}
      />
    );
  }

  // Success — minimal read-only rendering. Full widget rendering is left to
  // a follow-up once the backend payload shape is locked in Phase 4.10.
  // NB: the workbench sets `body { overflow: hidden }` globally — we
  // explicitly allow scroll here so long dashboards aren't clipped.
  return (
    <div className="h-dvh overflow-auto bg-zinc-50 dark:bg-zinc-950">
      <div className="mx-auto max-w-6xl px-6 py-8">
        <header className="border-b border-zinc-200 pb-4 dark:border-zinc-800">
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {state.payload.dashboard.name}
          </h1>
          {state.payload.dashboard.description && (
            <p className="mt-1 text-sm text-zinc-600 dark:text-zinc-400">
              {state.payload.dashboard.description}
            </p>
          )}
          <p className="mt-2 text-[10px] text-zinc-400">
            공유된 대시보드 · 읽기 전용 (Shared · read-only)
          </p>
        </header>

        <div className="mt-6 space-y-3">
          {state.payload.widgets.length === 0 ? (
            <p className="text-sm text-zinc-400">위젯이 없습니다.</p>
          ) : (
            state.payload.widgets.map((w) => (
              <div
                key={w.id}
                className="rounded-lg border border-zinc-200 bg-white p-4 dark:border-zinc-800 dark:bg-zinc-900"
              >
                <div className="flex items-center justify-between">
                  <h2 className="truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
                    {w.title}
                  </h2>
                  <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
                    {w.widget_type}
                  </span>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
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
    <div className="flex min-h-dvh items-center justify-center bg-zinc-50 px-4 dark:bg-zinc-950">
      <div className="w-full max-w-md rounded-xl border border-zinc-200 bg-white p-6 text-center shadow-sm dark:border-zinc-800 dark:bg-zinc-900">
        <h1 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
          {title}
        </h1>
        <p className="mt-1 text-xs text-zinc-400">{subtitle}</p>
        <p className="mt-4 text-sm text-zinc-600 dark:text-zinc-400">{body}</p>
      </div>
    </div>
  );
}

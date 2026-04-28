"use client";

import { Component, type ErrorInfo, type ReactNode } from "react";
import { useTranslations } from "next-intl";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
  name?: string;
}

interface State {
  hasError: boolean;
  error: Error | null;
  /** Incrementing key forces children to remount on recovery. */
  retryKey: number;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null, retryKey: 0 };
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error(
      `[ErrorBoundary${this.props.name ? `:${this.props.name}` : ""}]`,
      error,
      errorInfo.componentStack,
    );
  }

  private handleRetry = () => {
    this.setState((prev) => ({
      hasError: false,
      error: null,
      retryKey: prev.retryKey + 1,
    }));
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;
      return (
        <ErrorBoundaryFallback
          error={this.state.error}
          onRetry={this.handleRetry}
        />
      );
    }
    // key={retryKey} forces complete remount of children on recovery,
    // ensuring stale state from the crashed tree is discarded.
    // "contents" display ensures this wrapper doesn't break parent flex/grid layouts.
    return <div key={this.state.retryKey} style={{ display: "contents" }}>{this.props.children}</div>;
  }
}

function ErrorBoundaryFallback({
  error,
  onRetry,
}: {
  error: Error | null;
  onRetry: () => void;
}) {
  const t = useTranslations("errorBoundary");
  const detail = error?.message ?? "";
  const showDetail =
    process.env.NODE_ENV !== "production" && detail.length > 0;
  return (
    <div className="flex items-center justify-center p-8">
      <div className="max-w-md text-center">
        <p className="text-sm font-medium text-red-500 dark:text-red-400">
          {t("title")}
        </p>
        <p className="mt-2 text-xs text-zinc-600 dark:text-muted-foreground">
          {t("description")}
        </p>
        {showDetail && (
          <pre className="mt-3 overflow-x-auto whitespace-pre-wrap break-all rounded bg-zinc-100 px-2 py-1 text-left text-[10px] text-zinc-600 dark:bg-zinc-800/60 dark:text-zinc-400">
            {detail}
          </pre>
        )}
        <button
          className="mt-4 rounded bg-zinc-200 px-3 py-1 text-xs font-medium text-zinc-800 hover:bg-zinc-300 dark:bg-zinc-700 dark:text-zinc-200 dark:hover:bg-zinc-600"
          onClick={onRetry}
        >
          {t("retry")}
        </button>
      </div>
    </div>
  );
}

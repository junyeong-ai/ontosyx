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
        <p className="text-sm font-medium text-danger-foreground">
          {t("title")}
        </p>
        <p className="mt-2 text-xs text-foreground-muted">
          {t("description")}
        </p>
        {showDetail && (
          <pre className="mt-3 overflow-x-auto whitespace-pre-wrap break-all rounded bg-surface-inset px-2 py-1 text-start text-2xs text-foreground-muted">
            {detail}
          </pre>
        )}
        <button type="button"
          className="mt-4 rounded bg-surface-inset px-3 py-1 text-xs font-medium text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-raised"
          onClick={onRetry}
        >
          {t("retry")}
        </button>
      </div>
    </div>
  );
}

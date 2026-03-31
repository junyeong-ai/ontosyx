"use client";

import { Component, type ErrorInfo, type ReactNode } from "react";

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
        <div className="flex items-center justify-center p-8 text-sm text-red-400">
          <div className="text-center">
            <p className="font-medium">Something went wrong</p>
            <p className="mt-1 text-xs text-neutral-500">
              {this.state.error?.message ?? "Unknown error"}
            </p>
            <button
              className="mt-3 rounded bg-neutral-700 px-3 py-1 text-xs hover:bg-neutral-600"
              onClick={this.handleRetry}
            >
              Try again
            </button>
          </div>
        </div>
      );
    }
    // key={retryKey} forces complete remount of children on recovery,
    // ensuring stale state from the crashed tree is discarded.
    // "contents" display ensures this wrapper doesn't break parent flex/grid layouts.
    return <div key={this.state.retryKey} style={{ display: "contents" }}>{this.props.children}</div>;
  }
}

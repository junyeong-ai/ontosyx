"use client";

import { Component, type ErrorInfo, type ReactNode } from "react";
import { useTranslations } from "next-intl";

interface BoundaryProps {
  widgetType: string;
  children: ReactNode;
  titleLabel: string;
  retryLabel: string;
  unknownErrorLabel: string;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * Error boundary for individual widgets.
 *
 * If a chart or visualization crashes, only that widget shows a fallback —
 * the rest of the results panel stays intact.
 */
class WidgetErrorBoundaryInner extends Component<BoundaryProps, State> {
  constructor(props: BoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(
      `Widget "${this.props.widgetType}" crashed:`,
      error,
      info.componentStack,
    );
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="rounded-lg border border-danger-border bg-danger-surface p-4 text-sm">
          <p className="font-medium text-danger-foreground">
            {this.props.titleLabel}
          </p>
          <p className="mt-1 text-danger-foreground">
            {this.state.error?.message ?? this.props.unknownErrorLabel}
          </p>
          <button
            type="button"
            onClick={() => this.setState({ hasError: false, error: null })}
            className="mt-2 rounded bg-danger-surface px-3 py-1 text-xs font-medium text-danger-foreground hover:bg-danger-surface"
          >
            {this.props.retryLabel}
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

interface Props {
  widgetType: string;
  children: ReactNode;
}

/**
 * Wraps the class-based boundary with translated labels resolved at render time.
 * The boundary itself must stay a class component to intercept render errors.
 */
export function WidgetErrorBoundary({ widgetType, children }: Props) {
  const t = useTranslations("widget.errorBoundary");
  const tCommon = useTranslations("common");
  return (
    <WidgetErrorBoundaryInner
      widgetType={widgetType}
      titleLabel={t("title")}
      retryLabel={tCommon("retry")}
      unknownErrorLabel={t("unknownError")}
    >
      {children}
    </WidgetErrorBoundaryInner>
  );
}

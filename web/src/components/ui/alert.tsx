"use client";

import { HugeiconsIcon } from "@hugeicons/react";
import {
  Alert01Icon,
  InformationCircleIcon,
  CheckmarkCircle02Icon,
  Cancel01Icon,
} from "@hugeicons/core-free-icons";
import { cn } from "@/lib/cn";

type AlertVariant = "info" | "success" | "warning" | "error";

const variantStyles: Record<AlertVariant, { container: string; icon: string }> = {
  info: {
    container: "border-sky-200 bg-sky-50 text-sky-800 dark:border-sky-800 dark:bg-sky-950/40 dark:text-sky-300",
    icon: "text-sky-500",
  },
  success: {
    container: "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-300",
    icon: "text-emerald-500",
  },
  warning: {
    container: "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-300",
    icon: "text-amber-500",
  },
  error: {
    container: "border-red-200 bg-red-50 text-red-800 dark:border-red-800 dark:bg-red-950/40 dark:text-red-300",
    icon: "text-red-500",
  },
};

const variantIcons = {
  info: InformationCircleIcon,
  success: CheckmarkCircle02Icon,
  warning: Alert01Icon,
  error: Cancel01Icon,
};

interface AlertProps {
  variant?: AlertVariant;
  title?: string;
  children: React.ReactNode;
  className?: string;
  onDismiss?: () => void;
}

export function Alert({ variant = "info", title, children, className, onDismiss }: AlertProps) {
  const styles = variantStyles[variant];
  const Icon = variantIcons[variant];

  return (
    <div
      role="alert"
      className={cn(
        "flex gap-2.5 rounded-lg border p-3 text-sm",
        styles.container,
        className,
      )}
    >
      <HugeiconsIcon icon={Icon} className={cn("mt-0.5 h-4 w-4 shrink-0", styles.icon)} size="100%" />
      <div className="min-w-0 flex-1">
        {title && <p className="mb-0.5 font-semibold">{title}</p>}
        <div className="text-xs leading-relaxed">{children}</div>
      </div>
      {onDismiss && (
        <button
          onClick={onDismiss}
          className="shrink-0 rounded p-0.5 opacity-60 transition-opacity hover:opacity-100"
          aria-label="Dismiss"
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-3.5 w-3.5" size="100%" />
        </button>
      )}
    </div>
  );
}

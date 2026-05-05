"use client";

import { useTranslations } from "next-intl";
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
    container: "border-info-border bg-info-surface text-info-foreground",
    icon: "text-info-foreground",
  },
  success: {
    container:
      "border-brand-border bg-brand-surface text-brand-foreground-strong",
    icon: "text-brand-foreground",
  },
  warning: {
    container:
      "border-warning-border bg-warning-surface text-warning-foreground",
    icon: "text-warning-foreground",
  },
  error: {
    container: "border-danger-border bg-danger-surface text-danger-foreground",
    icon: "text-danger-foreground",
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

export function Alert({
  variant = "info",
  title,
  children,
  className,
  onDismiss,
}: AlertProps) {
  const t = useTranslations("common.alert");
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
      <HugeiconsIcon
        icon={Icon}
        className={cn("mt-0.5 h-4 w-4 shrink-0", styles.icon)}
        size="100%"
      />
      <div className="min-w-0 flex-1">
        {title && <p className="mb-0.5 font-semibold">{title}</p>}
        <div className="text-xs leading-relaxed">{children}</div>
      </div>
      {onDismiss && (
        <button
          type="button"
          onClick={onDismiss}
          className="shrink-0 rounded p-0.5 opacity-60 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:opacity-100"
          aria-label={t("dismiss")}
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-3.5 w-3.5" size="100%" />
        </button>
      )}
    </div>
  );
}

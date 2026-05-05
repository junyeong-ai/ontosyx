"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";

type SpinnerSize = "xs" | "sm" | "md" | "lg";

const sizeClasses: Record<SpinnerSize, string> = {
  xs: "h-3 w-3 border",
  sm: "h-4 w-4 border-[1.5px]",
  md: "h-5 w-5 border-2",
  lg: "h-6 w-6 border-2",
};

interface SpinnerProps {
  size?: SpinnerSize;
  className?: string;
}

export function Spinner({ size = "sm", className }: SpinnerProps) {
  const t = useTranslations("common");
  return (
    <span
      role="status"
      aria-label={t("loading")}
      className={cn(
        "inline-block animate-spin rounded-full border-current border-e-transparent",
        sizeClasses[size],
        className,
      )}
    />
  );
}

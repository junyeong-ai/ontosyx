"use client";

// `<ApiErrorState>` — branch ErrorState copy and recovery affordance
// on the semantic `ApiError.kind()`.
//
// Without this primitive, every panel that catches an `ApiError`
// renders the same generic "Something went wrong — try again" copy
// regardless of cause; a 401 looks identical to a 500 and the user
// retries forever instead of signing in. Foundry-class platforms
// branch the message register, the recovery affordance, and the
// recommended next action per status — that's what this primitive
// owns.
//
// Copy lives in i18n (`apiErrors.<kind>`). The host can override
// the title / description for a domain-specific phrasing
// ("Glossary term not found" instead of the generic "Resource not
// found") while still inheriting the kind-appropriate retry / sign-in
// affordance.

import { useTranslations } from "next-intl";
import { useRouter } from "next/navigation";

import { ErrorState } from "./error-state";
import { ApiError, type ApiErrorKind } from "@/lib/api/client";

export interface ApiErrorStateProps {
  error: unknown;
  /** Override the title; falls back to the kind-specific copy. */
  title?: string;
  /** Override the description; falls back to the kind-specific copy. */
  description?: string;
  /**
   * Retry handler. Hidden when the kind is non-retryable (`unauthorized`,
   * `forbidden`, `notFound`) — for those, the recovery affordance is
   * "sign in", "go home", or "request access", not "retry".
   */
  onRetry?: () => void;
}

const RETRYABLE_KINDS = new Set<ApiErrorKind>([
  "rateLimited",
  "serverError",
  "network",
  "unknown",
]);

export function ApiErrorState({
  error,
  title,
  description,
  onRetry,
}: ApiErrorStateProps) {
  const t = useTranslations("apiErrors");
  const tCommon = useTranslations("common");
  const router = useRouter();

  const kind: ApiErrorKind =
    error instanceof ApiError ? error.kind() : "unknown";

  const finalTitle = title ?? t(`${kind}.title`);
  const finalDescription = description ?? t(`${kind}.description`);

  // Sign-in CTA on 401: route to /login and round-trip back to the
  // current path. Skip the generic retry — clicking "Try again" on
  // an expired session just re-fires the failing call.
  if (kind === "unauthorized") {
    return (
      <ErrorState
        title={finalTitle}
        description={finalDescription}
        onRetry={() => {
          const next = encodeURIComponent(
            typeof window !== "undefined"
              ? window.location.pathname + window.location.search
              : "/",
          );
          router.push(`/login?next=${next}`);
        }}
        retryLabel={t("signIn")}
      />
    );
  }

  // Permission / not-found: no recovery from inside the panel, just
  // describe what happened. The host can layer a "Go home" CTA via
  // `onRetry` if it has somewhere meaningful to send the user.
  if (kind === "forbidden" || kind === "notFound") {
    return (
      <ErrorState
        title={finalTitle}
        description={finalDescription}
        onRetry={onRetry}
        retryLabel={onRetry ? tCommon("close") : undefined}
      />
    );
  }

  return (
    <ErrorState
      title={finalTitle}
      description={finalDescription}
      onRetry={RETRYABLE_KINDS.has(kind) ? onRetry : undefined}
      retryLabel={onRetry ? tCommon("retry") : undefined}
    />
  );
}

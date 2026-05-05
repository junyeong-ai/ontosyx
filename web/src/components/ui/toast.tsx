"use client";

/**
 * Design-system toast surface.
 *
 * Single entry point for transient notifications: `toast.<variant>()`
 * + the wrapped `<Toaster />` mount. Variants own the icon + status
 * colour; runtime placement / queue depth lives on the `<Toaster />`
 * element so callers don't repeat config. Direct `sonner` imports are
 * blocked at the lint layer to keep this the only surface.
 */

import {
  AlertCircleIcon,
  AlertDiamondIcon,
  CheckmarkCircle02Icon,
  InformationCircleIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Toaster as SonnerToaster,
  toast as sonnerToast,
  type ExternalToast,
} from "sonner";

type Variant = "success" | "error" | "warning" | "info";

const VARIANT_DEFINITION: Record<
  Variant,
  { icon: typeof CheckmarkCircle02Icon; tone: string }
> = {
  success: { icon: CheckmarkCircle02Icon, tone: "text-success-foreground" },
  error: { icon: AlertCircleIcon, tone: "text-danger-foreground" },
  warning: { icon: AlertDiamondIcon, tone: "text-warning-foreground" },
  info: { icon: InformationCircleIcon, tone: "text-info-foreground" },
};

function variantIcon(variant: Variant) {
  const { icon, tone } = VARIANT_DEFINITION[variant];
  return <HugeiconsIcon icon={icon} className={`size-5 ${tone}`} />;
}

function withDefaultIcon(
  variant: Variant,
  options?: ExternalToast,
): ExternalToast {
  if (options?.icon) return options;
  return { ...options, icon: variantIcon(variant) };
}

/**
 * Options for `toast.undoable` — a success toast that surfaces an
 * "Undo" CTA for `windowMs` (default 30s) before fading out.
 *
 * Pattern: surface this immediately after a destructive call returns
 * 200; if the user clicks Undo within the window, fire `onUndo`,
 * which should hit a restore endpoint or revert the local state.
 * The toast is the *only* affordance — there is no "are you sure?"
 * preceding the delete; the 30s window is the safety net. This is
 * the Linear / Gmail / Foundry pattern (one click to delete with
 * an undo lifeline) and is far less friction than a confirmation
 * dialog for actions the user does dozens of times per session.
 */
export interface UndoableToastOptions {
  message: string;
  onUndo: () => void | Promise<void>;
  /** Defaults to 30s — long enough for the "wait, that wasn't right" beat. */
  windowMs?: number;
  /** Override the action label. i18n at the call site. */
  undoLabel?: string;
  /** Optional description rendered below the headline. */
  description?: string;
}

export const toast = {
  success: (message: string, options?: ExternalToast) =>
    sonnerToast.success(message, withDefaultIcon("success", options)),
  error: (message: string, options?: ExternalToast) =>
    sonnerToast.error(message, withDefaultIcon("error", options)),
  warning: (message: string, options?: ExternalToast) =>
    sonnerToast.warning(message, withDefaultIcon("warning", options)),
  info: (message: string, options?: ExternalToast) =>
    sonnerToast.info(message, withDefaultIcon("info", options)),
  /** Neutral notification — no semantic colour. */
  message: (message: string, options?: ExternalToast) =>
    sonnerToast(message, options),
  /** Indeterminate progress; pair with `toast.dismiss(id)` on resolve. */
  loading: (message: string, options?: ExternalToast) =>
    sonnerToast.loading(message, options),
  /**
   * Undoable success toast. Surface after a destructive operation
   * succeeds; the user has `windowMs` to take it back via the action
   * button. Returns the toast id so the caller can `dismiss()` early
   * (e.g. on route change or workspace switch where the undo target
   * is no longer valid).
   */
  undoable: ({
    message,
    onUndo,
    windowMs = 30_000,
    undoLabel = "Undo",
    description,
  }: UndoableToastOptions) =>
    sonnerToast.success(
      message,
      withDefaultIcon("success", {
        duration: windowMs,
        description,
        action: {
          label: undoLabel,
          onClick: () => {
            void onUndo();
          },
        },
      }),
    ),
  dismiss: sonnerToast.dismiss,
  promise: sonnerToast.promise,
};

/**
 * Toaster mount. Layouts render one `<Toaster />` and inherit shared
 * placement, density, and queue behaviour. Local overrides are
 * discouraged — drift the wrapper instead.
 */
export function Toaster() {
  return (
    <SonnerToaster
      position="bottom-right"
      // Cap the visible stack so a burst of background activity doesn't
      // tower into the corner. Older messages roll off the bottom.
      visibleToasts={3}
      // Tactile dismissal: pull-to-the-right closes a toast on touch
      // and trackpad. Keyboard users still dismiss via the close button.
      closeButton
      // Long enough to read a description; short enough that a success
      // toast doesn't linger past the next click.
      duration={4500}
      // sonner uses inline styles; `className` extension keeps
      // typography in sync with the rest of the design system.
      toastOptions={{ className: "text-sm" }}
    />
  );
}

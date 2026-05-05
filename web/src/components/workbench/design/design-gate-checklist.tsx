"use client";

import { useCallback } from "react";
import { useTranslations } from "next-intl";
import { AlertCircle, X } from "lucide-react";
import { CheckCircle } from "lucide-react";
import { cn } from "@/lib/cn";
import type { DesignGate, GateId } from "@/types/ontology-drafts";

/**
 * Sticky checklist of every server-evaluated gate the operator has
 * to satisfy before the "온톨로지 설계" action becomes available.
 *
 * The wire shape is the single source of truth: backend computes the
 * `Vec<DesignGate>` via `evaluate_design_gates`, FE renders one row
 * per gate with i18n copy keyed by `gate.id` and parameters
 * interpolated from `gate.params`. Click on an unmet gate scrolls the
 * page to the inline control referenced by `gate.anchor` and fires a
 * one-shot pulse so the eye finds the next thing to fix.
 *
 * The component renders exactly the gates the server returns — there
 * is no FE-side filtering, ordering, or precondition logic. New gate
 * variants need a new `gate.${id}` i18n entry; the UI picks them up
 * automatically.
 */
export function DesignGateChecklist({
  gates,
  className,
}: {
  gates: ReadonlyArray<DesignGate>;
  className?: string;
}) {
  const t = useTranslations("workbench.bottomPanel.designGates");

  const focusGate = useCallback((gate: DesignGate) => {
    if (!gate.anchor) return;
    const el =
      typeof document !== "undefined"
        ? document.getElementById(gate.anchor)
        : null;
    if (!el) return;
    el.scrollIntoView({ behavior: "smooth", block: "center" });
    // One-shot emerald pulse so the eye lands on the control that
    // resolves the gate. Tailwind doesn't ship a built-in "pulse
    // ring" so we use a transient utility class added via JS — no
    // CSS leaks across the app, the class self-cleans after 1.2s.
    el.classList.add("ring-2", "ring-brand-foreground", "ring-offset-2");
    window.setTimeout(() => {
      el.classList.remove("ring-2", "ring-brand-foreground", "ring-offset-2");
    }, 1200);
  }, []);

  if (gates.length === 0) return null;

  const unmetCount = gates.filter(
    (g) => g.status === "unmet" && g.blocks_design,
  ).length;
  const metCount = gates.filter((g) => g.status === "met").length;

  return (
    <div
      role="status"
      aria-live="polite"
      className={cn(
        "rounded-md border border-divider bg-surface-base p-3",
        className,
      )}
    >
      <header className="mb-2 flex items-baseline justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-foreground-strong">
          {t("heading")}
        </h3>
        <span
          className={cn(
            "text-xs font-medium",
            unmetCount === 0
              ? "text-brand-foreground"
              : "text-warning-foreground",
          )}
        >
          {t("progress", { met: metCount, total: gates.length })}
        </span>
      </header>
      <ul className="flex flex-col gap-1.5">
        {gates.map((gate) => (
          <GateRow key={gate.id} gate={gate} onFocus={focusGate} t={t} />
        ))}
      </ul>
    </div>
  );
}

function GateRow({
  gate,
  onFocus,
  t,
}: {
  gate: DesignGate;
  onFocus: (gate: DesignGate) => void;
  t: ReturnType<typeof useTranslations>;
}) {
  const isMet = gate.status === "met";
  const isBlocking = gate.blocks_design;
  const isUnmetBlocker = !isMet && isBlocking;

  return (
    <li>
      <button
        type="button"
        onClick={() => onFocus(gate)}
        disabled={!gate.anchor}
        className={cn(
          "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
          "hover:bg-surface-raised",
          "disabled:cursor-default disabled:hover:bg-transparent",
          isUnmetBlocker && "border border-warning-border",
        )}
        aria-label={t(`label.${gate.id}` as MessageKey, {
          ...(gate.params ?? {}),
        })}
      >
        <GateIcon status={gate.status} blocking={isBlocking} />
        <div className="flex min-w-0 flex-1 flex-col">
          <span
            className={cn(
              "text-xs",
              isMet
                ? "text-foreground line-through decoration-brand-foreground-subtle"
                : "text-foreground-strong",
            )}
          >
            {t(`label.${gate.id}` as MessageKey, {
              ...(gate.params ?? {}),
            })}
          </span>
          {!isMet && (
            <span className="text-xs text-foreground-muted">
              {t(`hint.${gate.id}` as MessageKey, {
                ...(gate.params ?? {}),
              })}
            </span>
          )}
        </div>
      </button>
    </li>
  );
}

function GateIcon({
  status,
  blocking,
}: {
  status: "met" | "unmet";
  blocking: boolean;
}) {
  if (status === "met") {
    return (
      <CheckCircle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-brand-foreground" />
    );
  }
  if (blocking) {
    return (
      <X className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning-foreground" />
    );
  }
  return (
    <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-foreground-subtle" />
  );
}

// Type-level guarantee that gate-id-suffixed message keys exist. The
// next-intl `t` overload insists on a concrete literal — we pass a
// dynamic string built from `gate.id`, so we widen the message-key
// type at the call site rather than spreading `as any` casts.
type MessageKey =
  | `label.${GateId}`
  | `hint.${GateId}`;

/**
 * Imperatively scroll to the first unmet blocking gate and trigger a
 * one-shot pulse on the inline control. Wired into the
 * design-button onClick path so a click on a disabled button still
 * communicates "this is what's missing" rather than silently no-op.
 */
export function focusFirstUnmetGate(gates: ReadonlyArray<DesignGate>): void {
  const target = gates.find(
    (g) => g.blocks_design && g.status === "unmet" && !!g.anchor,
  );
  if (!target?.anchor) return;
  const el =
    typeof document !== "undefined" ? document.getElementById(target.anchor) : null;
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.add("ring-2", "ring-warning-foreground", "ring-offset-2");
  window.setTimeout(() => {
    el.classList.remove("ring-2", "ring-warning-foreground", "ring-offset-2");
  }, 1200);
}

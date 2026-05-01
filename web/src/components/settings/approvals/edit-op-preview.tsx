"use client";

import { useTranslations } from "next-intl";

import type { OntologyEditOp } from "@/lib/api/edit-ops";

interface EditOpPreviewProps {
  /** The full approval payload (`EditOntologyRequest` shape). The
   *  payload is `unknown` because the backend stores it as
   *  `serde_json::Value`; we narrow inside the component so a
   *  malformed row degrades to a "raw payload" fallback rather
   *  than crashing the page. */
  payload: unknown;
}

interface ParsedPayload {
  operations: OntologyEditOp[];
  expected_version?: number;
  message?: string;
  dry_run?: boolean;
}

/** Best-effort parse of the JSON value into the EditOntologyRequest
 *  shape. Returns `null` when the payload doesn't carry an
 *  operations array — caller falls back to a raw JSON dump. */
function parsePayload(payload: unknown): ParsedPayload | null {
  if (typeof payload !== "object" || payload === null) return null;
  const obj = payload as Record<string, unknown>;
  if (!Array.isArray(obj.operations)) return null;
  return {
    operations: obj.operations as OntologyEditOp[],
    expected_version:
      typeof obj.expected_version === "number" ? obj.expected_version : undefined,
    message: typeof obj.message === "string" ? obj.message : undefined,
    dry_run: typeof obj.dry_run === "boolean" ? obj.dry_run : undefined,
  };
}

type OpKind = "create" | "update" | "delete" | "bind" | "deprecate";

function classifyOp(op: OntologyEditOp): OpKind {
  const tag = op.op;
  if (tag.startsWith("create_")) return "create";
  if (tag.startsWith("update_")) return "update";
  if (tag.startsWith("delete_")) return "delete";
  if (tag.startsWith("deprecate_")) return "deprecate";
  return "bind";
}

const KIND_STYLES: Record<OpKind, string> = {
  create:
    "bg-emerald-100 text-emerald-700 border-emerald-300 dark:bg-emerald-900/30 dark:text-emerald-300 dark:border-emerald-800",
  update:
    "bg-amber-100 text-amber-700 border-amber-300 dark:bg-amber-900/30 dark:text-amber-300 dark:border-amber-800",
  delete:
    "bg-rose-100 text-rose-700 border-rose-300 dark:bg-rose-900/30 dark:text-rose-300 dark:border-rose-800",
  deprecate:
    "bg-violet-100 text-violet-700 border-violet-300 dark:bg-violet-900/30 dark:text-violet-300 dark:border-violet-800",
  bind: "bg-sky-100 text-sky-700 border-sky-300 dark:bg-sky-900/30 dark:text-sky-300 dark:border-sky-800",
};

/** Pull a short identifier out of an op for the chip label. Each
 *  variant carries a different reference shape; missing fields
 *  fall back to "—" so the chip still renders. */
function opSummary(op: OntologyEditOp): string {
  const r = op as Record<string, unknown>;
  // create / update / delete generally carry `id` or `def.id` or
  // `mapping.id` or `value.id`.
  const id =
    (r.id as string | undefined) ??
    ((r.def as Record<string, unknown> | undefined)?.id as string | undefined) ??
    ((r.mapping as Record<string, unknown> | undefined)?.id as
      | string
      | undefined) ??
    ((r.value as Record<string, unknown> | undefined)?.id as
      | string
      | undefined) ??
    "—";
  return id;
}

export function EditOpPreview({ payload }: EditOpPreviewProps) {
  const t = useTranslations("settings.approvals.preview");
  const parsed = parsePayload(payload);

  if (!parsed) {
    // Non-edit payload (or malformed). Surface as raw JSON so a
    // reviewer still has *something* to read instead of nothing.
    return (
      <pre className="mt-2 max-h-64 overflow-auto rounded border border-zinc-200 bg-zinc-50 p-2 text-[10px] text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
        {JSON.stringify(payload, null, 2)}
      </pre>
    );
  }

  const counts = parsed.operations.reduce<Record<OpKind, number>>(
    (acc, op) => {
      const kind = classifyOp(op);
      acc[kind] = (acc[kind] ?? 0) + 1;
      return acc;
    },
    { create: 0, update: 0, delete: 0, deprecate: 0, bind: 0 },
  );

  return (
    <div className="mt-3 flex flex-col gap-2">
      {/* Summary row — one line, scannable at a glance. */}
      <div className="flex items-center gap-2 flex-wrap text-xs">
        <span className="font-medium text-zinc-700 dark:text-zinc-300">
          {t("summary", { count: parsed.operations.length })}
        </span>
        {(["create", "update", "delete", "deprecate", "bind"] as const).map(
          (kind) =>
            counts[kind] > 0 && (
              <span
                key={kind}
                className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${KIND_STYLES[kind]}`}
              >
                {t(`kinds.${kind}`, { count: counts[kind] })}
              </span>
            ),
        )}
        {parsed.expected_version !== undefined && (
          <span className="text-[10px] text-muted-foreground">
            {t("expectedVersion", { v: parsed.expected_version })}
          </span>
        )}
      </div>

      {/* Operation list — full chip per op so the reviewer can
          spot unexpected entries. Capped at 50 to keep the row
          height bounded; truncation marker fires above that. */}
      <ul className="flex flex-col gap-1">
        {parsed.operations.slice(0, 50).map((op, i) => {
          const kind = classifyOp(op);
          return (
            <li
              key={i}
              className="flex items-center gap-2 rounded border border-zinc-200 bg-white px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900"
            >
              <span
                className={`shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-mono ${KIND_STYLES[kind]}`}
              >
                {op.op}
              </span>
              <span className="truncate font-mono text-[11px] text-zinc-700 dark:text-zinc-300">
                {opSummary(op)}
              </span>
            </li>
          );
        })}
        {parsed.operations.length > 50 && (
          <li className="text-[10px] italic text-muted-foreground">
            {t("truncated", { remaining: parsed.operations.length - 50 })}
          </li>
        )}
      </ul>

      {parsed.message && (
        <p className="rounded border border-zinc-200 bg-zinc-50 p-2 text-[11px] text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
          <span className="font-medium">{t("messageLabel")}:</span>{" "}
          {parsed.message}
        </p>
      )}
    </div>
  );
}

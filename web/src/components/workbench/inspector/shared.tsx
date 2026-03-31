"use client";

import type { ConstraintDef, NodeTypeDef } from "@/types/api";

// ---------------------------------------------------------------------------
// Shared section wrapper
// ---------------------------------------------------------------------------

export function Section({
  title,
  action,
  children,
}: {
  title: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-zinc-200 dark:border-zinc-800">
      <div className="flex items-center justify-between bg-zinc-50 px-3 py-1 dark:bg-zinc-900">
        <span className="font-semibold uppercase tracking-wider text-zinc-500">
          {title}
        </span>
        {action && <div className="flex items-center gap-0.5">{action}</div>}
      </div>
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Constraint formatter
// ---------------------------------------------------------------------------

export function formatConstraint(
  cd: ConstraintDef,
  node: NodeTypeDef,
): string {
  const resolveName = (pid: string) =>
    node.properties.find((p) => p.id === pid)?.name ?? pid;

  switch (cd.type) {
    case "unique":
      return `UNIQUE(${(cd.property_ids ?? []).map(resolveName).join(", ")})`;
    case "exists":
      return `EXISTS(${resolveName(cd.property_id ?? "")})`;
    case "node_key":
      return `NODE KEY(${(cd.property_ids ?? []).map(resolveName).join(", ")})`;
    default:
      return String((cd as { type: string }).type);
  }
}

"use client";

import type { ConstraintDef, NodeTypeDef } from "@/types/api";
import { arr } from "@/lib/ir-collections";

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
    <div className="border-b border-divider">
      <div className="flex items-center justify-between bg-surface-raised px-3 py-1">
        <span className="font-semibold uppercase tracking-wider text-muted-foreground">
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
    arr(node.properties).find((p) => p.id === pid)?.name ?? pid;

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

"use client";

// `<CommandStackDiffDialog>` — explicit reconciliation surface for
// a `MergeBanner` conflict.
//
// The banner attributes the conflict to one user and offers Keep
// mine / Take theirs. `<CommandStackDiffDialog>` is the
// information-rich third surface: a list of every local op the
// user is about to either preserve or discard, together with the
// remote revision that arrived. The user picks the resolution with
// full inventory in front of them — no "what was I editing again?"
// memory load.
//
// Why a dedicated dialog rather than expanding the banner: the
// banner sits inside the inspector body where vertical real-estate
// is precious. A 12-op edit stack would push the form chrome below
// the fold and the user would scroll through the conflict surface
// to reach the entity they care about. Keeping it modal means the
// user reads the inventory once, decides, and resumes editing.
//
// The dialog is read-only — the *resolution* still flows through
// the same `Keep mine` / `Take theirs` callbacks the banner uses,
// so the host owns one reconciliation pathway. Surface mounts only
// when the host passes `onCompare` to `<MergeBanner>`; absent that,
// the affordance is hidden.

import { useTranslations } from "next-intl";

import { Modal } from "@/components/ui/modal";
import { Button } from "@/components/ui/button";
import { commandOpBadge, formatCommand } from "@/lib/command-format";
import type { FormattableOntologyCommand } from "@/lib/command-format";
import { cn } from "@/lib/cn";
import type { OntologyIR } from "@/types/api";
import type { CommandEntry } from "@/lib/store";
import type { WireOntologyCommand } from "@/lib/collab/types";

export interface CommandStackDiffDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Draft ontology in its server-canonical (pre-rebase) shape. */
  ontology: OntologyIR | null;
  /** Server revision number that the local stack was authored against. */
  baseRevision: number;
  /** Server revision number that arrived mid-session and triggered the conflict. */
  remoteRevision: number;
  /** Display name of the remote update author. */
  remoteAuthorName: string;
  /** Local edit stack — the user's pending ops, oldest first. */
  commandStack: readonly CommandEntry[];
  /**
   * Remote ops applied between `baseRevision` → `remoteRevision`,
   * oldest first. Populated when the host received an
   * `EntityUpdated` WebSocket event carrying the per-revision
   * command delta; left `undefined` when the BE only published the
   * revision bump without the inventory. The dialog renders the
   * symmetric inventory when supplied and falls back to an opaque
   * "remote arrived" message otherwise.
   */
  remoteCommands?: readonly WireOntologyCommand[];
  /** Resolve by rebasing local stack atop the remote. */
  onKeepLocal: () => void;
  /** Resolve by dropping local stack and accepting the remote. */
  onAcceptRemote: () => void;
  /** True while a resolve is in flight; disables the action buttons. */
  busy?: boolean;
}

export function CommandStackDiffDialog({
  open,
  onOpenChange,
  ontology,
  baseRevision,
  remoteRevision,
  remoteAuthorName,
  commandStack,
  remoteCommands,
  onKeepLocal,
  onAcceptRemote,
  busy = false,
}: CommandStackDiffDialogProps) {
  const t = useTranslations("collab.commandStackDiff");
  const tCommand = useTranslations(
    "workbench.canvas.commandPreview.command",
  );

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t("title")}
      description={t("description", {
        author: remoteAuthorName,
        baseRevision,
        remoteRevision,
      })}
      size="xl"
    >
      <div className="flex flex-col gap-4">
        {/* Local edit summary */}
        <section
          aria-labelledby="cs-diff-local-heading"
          className="rounded-lg border border-divider bg-surface-raised"
        >
          <header className="flex items-center justify-between border-b border-divider px-4 py-2.5">
            <h3
              id="cs-diff-local-heading"
              className="text-sm font-semibold text-foreground-strong"
            >
              {t("localHeading", { count: commandStack.length })}
            </h3>
            <span className="rounded-md bg-brand-surface px-2 py-0.5 font-mono text-2xs text-brand-foreground">
              v{baseRevision} + {commandStack.length}
            </span>
          </header>
          {commandStack.length === 0 ? (
            <p className="px-4 py-6 text-center text-xs text-foreground-muted">
              {t("localEmpty")}
            </p>
          ) : (
            <ol className="divide-y divide-divider">
              {commandStack.map((entry, idx) => (
                <CommandRow
                  key={`${idx}-${commandKey(entry.command)}`}
                  index={idx}
                  command={entry.command}
                  ontology={ontology}
                  tCommand={tCommand}
                />
              ))}
            </ol>
          )}
        </section>

        {/* Remote arrival summary */}
        <section
          aria-labelledby="cs-diff-remote-heading"
          className="rounded-lg border border-divider bg-surface-raised"
        >
          <header className="flex items-center justify-between border-b border-divider px-4 py-2.5">
            <h3
              id="cs-diff-remote-heading"
              className="text-sm font-semibold text-foreground-strong"
            >
              {t("remoteHeading", { author: remoteAuthorName })}
            </h3>
            <span className="rounded-md bg-warning-surface px-2 py-0.5 font-mono text-2xs text-warning-foreground">
              v{remoteRevision}
            </span>
          </header>
          {remoteCommands && remoteCommands.length > 0 ? (
            <ol className="divide-y divide-divider">
              {remoteCommands.map((command, idx) => (
                <CommandRow
                  key={`remote-${idx}-${commandKey(command)}`}
                  index={idx}
                  command={command}
                  ontology={ontology}
                  tCommand={tCommand}
                />
              ))}
            </ol>
          ) : (
            <p className="px-4 py-3 text-xs text-foreground-muted">
              {t("remoteOpaque")}
            </p>
          )}
        </section>

        {/* Actions — same callbacks as the banner; surface duplicates
            them so the user resolves from the same dialog they
            inspected. */}
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button
            variant="ghost"
            size="md"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            {t("close")}
          </Button>
          <Button
            variant="ghost"
            size="md"
            onClick={onAcceptRemote}
            disabled={busy}
          >
            {t("acceptRemote")}
          </Button>
          <Button
            variant="primary"
            size="md"
            onClick={onKeepLocal}
            disabled={busy}
          >
            {t("keepLocal")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Per-row rendering — pulls `formatCommand` + `commandOpBadge` so
// the diff dialog reads with the same affordance vocabulary as the
// canvas command preview, instead of inventing a parallel one.
// ---------------------------------------------------------------------------

function CommandRow({
  index,
  command,
  ontology,
  tCommand,
}: {
  index: number;
  command: FormattableOntologyCommand;
  ontology: OntologyIR | null;
  tCommand: (k: string, params?: Record<string, string | number>) => string;
}) {
  const formatted = formatCommand(command, ontology);
  const badge = commandOpBadge(command);
  const toneClass = BADGE_TONE[badge.color];
  return (
    <li className="flex items-start gap-3 px-4 py-2.5">
      <span className="font-mono text-2xs text-foreground-subtle">
        {String(index + 1).padStart(2, "0")}
      </span>
      <span
        className={cn(
          "rounded px-1.5 py-0.5 text-2xs font-semibold uppercase tracking-wider",
          toneClass,
        )}
      >
        {badge.label}
      </span>
      <span className="flex-1 text-xs text-foreground">
        {tCommand(formatted.key, formatted.params)}
      </span>
    </li>
  );
}

const BADGE_TONE: Record<
  ReturnType<typeof commandOpBadge>["color"],
  string
> = {
  green: "bg-brand-surface text-brand-foreground",
  red: "bg-danger-surface text-danger-foreground",
  blue: "bg-info-surface text-info-foreground",
};

/**
 * Stable per-command key suffix used to keep React's reconciliation
 * happy — two `add_node` commands at different positions still
 * collide if we key on the op name alone, so we splice in any
 * structural identifier the variant carries.
 */
function commandKey(command: FormattableOntologyCommand): string {
  if ("node_id" in command && command.node_id) return `${command.op}-${command.node_id}`;
  if ("edge_id" in command && command.edge_id) return `${command.op}-${command.edge_id}`;
  if ("id" in command && command.id) return `${command.op}-${String(command.id)}`;
  return command.op;
}

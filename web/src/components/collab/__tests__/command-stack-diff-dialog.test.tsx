import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import { CommandStackDiffDialog } from "../command-stack-diff-dialog";
import type { CommandEntry } from "@/lib/store";
import type { OntologyIR } from "@/types/api";

const messages = {
  collab: {
    commandStackDiff: {
      title: "Resolve merge conflict",
      description:
        "{author} pushed revision v{remoteRevision} while you were editing on top of v{baseRevision}. Review the operations below before resolving.",
      localHeading: "Your unsaved operations  ({count})",
      localEmpty: "No pending local operations.",
      remoteHeading: "Remote update from {author}",
      remoteOpaque:
        "The remote diff isn't surfaced inline yet. Resolve based on your local stack and the revision delta.",
      keepLocal: "Keep my edits",
      acceptRemote: "Take theirs",
      close: "Close",
    },
  },
  workbench: {
    canvas: {
      commandPreview: {
        command: {
          addNode: "Add node {label}",
          renameNode: "Rename {label} → {newLabel}",
        },
      },
    },
  },
};

function Wrapper({ children }: { children: React.ReactNode }) {
  return (
    <NextIntlClientProvider locale="en" messages={messages}>
      {children}
    </NextIntlClientProvider>
  );
}

const ONT: OntologyIR = {
  id: "ont",
  name: "Test",
  description: { default: "" },
  version: { number: 1 },
  node_types: [{ id: "n1", label: "Person", description: { default: "" }, properties: [] }],
  edge_types: [],
};

const STACK: CommandEntry[] = [
  {
    command: { op: "add_node", id: "new", label: "Order" },
    inverse: { op: "delete_node", node_id: "new" },
  },
  {
    command: { op: "rename_node", node_id: "n1", new_label: "Customer" },
    inverse: { op: "rename_node", node_id: "n1", new_label: "Person" },
  },
];

describe("CommandStackDiffDialog", () => {
  it("renders the title with revision context", () => {
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={5}
          remoteRevision={6}
          remoteAuthorName="Hyejin"
          commandStack={STACK}
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(screen.getByText(/Resolve merge conflict/)).toBeTruthy();
    // The author + revision numbers appear in description and badges.
    expect(screen.getAllByText(/Hyejin/).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText(/v5/).length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText(/v6/).length).toBeGreaterThanOrEqual(2);
  });

  it("lists every local op with its formatted label", () => {
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={STACK}
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(screen.getByText("Add node Order")).toBeTruthy();
    expect(screen.getByText("Rename Person → Customer")).toBeTruthy();
  });

  it("renders the empty-state message when the local stack is empty", () => {
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={[]}
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(screen.getByText("No pending local operations.")).toBeTruthy();
  });

  it("Close button toggles open state through onOpenChange", () => {
    const onOpenChange = vi.fn();
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={onOpenChange}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={STACK}
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    fireEvent.click(screen.getByText("Close"));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("Keep / Accept buttons fire the supplied callbacks", () => {
    const onKeep = vi.fn();
    const onAccept = vi.fn();
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={STACK}
          onKeepLocal={onKeep}
          onAcceptRemote={onAccept}
        />
      </Wrapper>,
    );
    fireEvent.click(screen.getByText("Keep my edits"));
    expect(onKeep).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByText("Take theirs"));
    expect(onAccept).toHaveBeenCalledTimes(1);
  });

  it("falls back to the opaque copy when remoteCommands is absent", () => {
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={STACK}
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(
      screen.getByText(/remote diff isn't surfaced inline yet/),
    ).toBeTruthy();
  });

  it("renders the symmetric remote inventory when remoteCommands is supplied", () => {
    const REMOTE: { op: "add_node"; id: string; label: string }[] = [
      { op: "add_node", id: "remote-1", label: "Refund" },
    ];
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={STACK}
          remoteCommands={REMOTE}
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    // The remote inventory replaces the opaque message with a
    // formatted op row mirroring the local list shape.
    expect(screen.getByText("Add node Refund")).toBeTruthy();
    expect(
      screen.queryByText(/remote diff isn't surfaced inline yet/),
    ).toBeNull();
  });

  it("disables every action when busy", () => {
    const onKeep = vi.fn();
    render(
      <Wrapper>
        <CommandStackDiffDialog
          open
          onOpenChange={() => {}}
          ontology={ONT}
          baseRevision={1}
          remoteRevision={2}
          remoteAuthorName="A"
          commandStack={STACK}
          onKeepLocal={onKeep}
          onAcceptRemote={() => {}}
          busy
        />
      </Wrapper>,
    );
    const keep = screen.getByRole("button", { name: "Keep my edits" });
    expect((keep as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(keep);
    expect(onKeep).not.toHaveBeenCalled();
  });
});

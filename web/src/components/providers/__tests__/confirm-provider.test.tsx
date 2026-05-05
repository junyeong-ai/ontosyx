import { describe, expect, it } from "vitest";
import { act, render, fireEvent, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import { ConfirmProvider, useConfirm } from "../confirm-provider";

const messages = {
  common: {
    cancel: "Cancel",
    confirm: "Confirm",
    delete: "Delete",
  },
};

function Wrapper({ children }: { children: React.ReactNode }) {
  return (
    <NextIntlClientProvider locale="en" messages={messages}>
      <ConfirmProvider>{children}</ConfirmProvider>
    </NextIntlClientProvider>
  );
}

function ConfirmHarness({
  resolved,
  options,
}: {
  resolved: (value: boolean) => void;
  options: Parameters<ReturnType<typeof useConfirm>>[0];
}) {
  const confirm = useConfirm();
  return (
    <button
      type="button"
      onClick={async () => {
        const ok = await confirm(options);
        resolved(ok);
      }}
    >
      open
    </button>
  );
}

describe("ConfirmProvider — typed-name gate", () => {
  it("disables Confirm until the typed phrase matches verbatim", async () => {
    const resolved: { value: boolean | undefined } = { value: undefined };
    render(
      <Wrapper>
        <ConfirmHarness
          resolved={(v) => {
            resolved.value = v;
          }}
          options={{
            title: "Delete project",
            description: "Type the project name to confirm.",
            confirmLabel: "Delete",
            variant: "danger",
            typeToConfirm: {
              phrase: "ontosyx",
              label: "Project name",
            },
          }}
        />
      </Wrapper>,
    );

    fireEvent.click(screen.getByText("open"));
    const confirmButton = await screen.findByRole("button", {
      name: "Delete",
    });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(true);

    const input = screen.getByPlaceholderText("ontosyx") as HTMLInputElement;

    // Partial match — still disabled.
    fireEvent.change(input, { target: { value: "onto" } });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(true);

    // Wrong case — still disabled (case-sensitive).
    fireEvent.change(input, { target: { value: "Ontosyx" } });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(true);

    // Exact match — enabled.
    fireEvent.change(input, { target: { value: "ontosyx" } });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(false);

    await act(async () => {
      fireEvent.click(confirmButton);
    });
    expect(resolved.value).toBe(true);
  });

  it("a normal confirm (no typeToConfirm) leaves the button enabled", async () => {
    const resolved: { value: boolean | undefined } = { value: undefined };
    render(
      <Wrapper>
        <ConfirmHarness
          resolved={(v) => {
            resolved.value = v;
          }}
          options={{
            title: "Discard changes",
            description: "Unsaved edits will be lost.",
          }}
        />
      </Wrapper>,
    );

    fireEvent.click(screen.getByText("open"));
    const confirmButton = await screen.findByRole("button", {
      name: "Confirm",
    });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(false);
  });

  it("Cancel resolves the promise with false even when type-gate is active", async () => {
    const resolved: { value: boolean | undefined } = { value: undefined };
    render(
      <Wrapper>
        <ConfirmHarness
          resolved={(v) => {
            resolved.value = v;
          }}
          options={{
            title: "Drop ontology",
            description: "Type the ontology name to confirm.",
            typeToConfirm: { phrase: "x", label: "Name" },
          }}
        />
      </Wrapper>,
    );

    fireEvent.click(screen.getByText("open"));
    const cancelBtn = await screen.findByRole("button", { name: "Cancel" });
    await act(async () => {
      fireEvent.click(cancelBtn);
    });
    expect(resolved.value).toBe(false);
  });
});

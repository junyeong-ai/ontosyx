import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { SaveBar } from "@/components/ui/save-bar";

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("SaveBar visibility", () => {
  it("hides itself when the form is clean and idle", () => {
    wrap(
      <SaveBar dirty={false} pending={false} onSave={vi.fn()} onDiscard={vi.fn()} />,
    );
    // Buttons render but the wrapper is aria-hidden + opacity-0 so the
    // bar reads as hidden to AT.
    const wrapper = screen.getByText("Save").closest('div[aria-hidden]');
    expect(wrapper).toHaveAttribute("aria-hidden", "true");
  });

  it("shows itself when the form is dirty", () => {
    wrap(
      <SaveBar dirty pending={false} onSave={vi.fn()} onDiscard={vi.fn()} />,
    );
    const wrapper = screen.getByText("Save").closest('div[aria-hidden]');
    expect(wrapper).toHaveAttribute("aria-hidden", "false");
    expect(screen.getByText(/Unsaved changes/i)).toBeInTheDocument();
  });

  it("shows the saving status while pending even if not dirty", () => {
    wrap(
      <SaveBar dirty={false} pending onSave={vi.fn()} onDiscard={vi.fn()} />,
    );
    expect(screen.getByText(/Saving/i)).toBeInTheDocument();
  });
});

describe("SaveBar actions", () => {
  it("Save button calls onSave when dirty", () => {
    const onSave = vi.fn();
    wrap(
      <SaveBar dirty pending={false} onSave={onSave} onDiscard={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("Discard button calls onDiscard when dirty", () => {
    const onDiscard = vi.fn();
    wrap(
      <SaveBar dirty pending={false} onSave={vi.fn()} onDiscard={onDiscard} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /discard/i }));
    expect(onDiscard).toHaveBeenCalledTimes(1);
  });

  it("Save and Discard are disabled while pending", () => {
    wrap(
      <SaveBar dirty pending onSave={vi.fn()} onDiscard={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: /save/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /discard/i })).toBeDisabled();
  });

  it("Save and Discard are disabled when not dirty (no work to do)", () => {
    wrap(
      <SaveBar dirty={false} pending={false} onSave={vi.fn()} onDiscard={vi.fn()} />,
    );
    // When the bar is not visible, its wrapper is `aria-hidden=true`,
    // so testing-library hides the buttons from the role query. Reach
    // by text content + check `disabled` directly.
    const saveBtn = screen.getByText("Save").closest("button");
    const discardBtn = screen.getByText("Discard").closest("button");
    expect(saveBtn).toBeDisabled();
    expect(discardBtn).toBeDisabled();
  });
});

describe("SaveBar relative timestamp", () => {
  it("renders 'just now' for a sub-minute lastSavedAt", () => {
    const tenSecondsAgo = new Date(Date.now() - 10_000).toISOString();
    wrap(
      <SaveBar
        dirty={false}
        pending={false}
        lastSavedAt={tenSecondsAgo}
        onSave={vi.fn()}
        onDiscard={vi.fn()}
      />,
    );
    expect(screen.getByText(/just now/i)).toBeInTheDocument();
  });

  it("renders minutes-ago when between 1m and 1h", () => {
    const tenMinutesAgo = new Date(Date.now() - 10 * 60_000).toISOString();
    wrap(
      <SaveBar
        dirty={false}
        pending={false}
        lastSavedAt={tenMinutesAgo}
        onSave={vi.fn()}
        onDiscard={vi.fn()}
      />,
    );
    expect(screen.getByText(/10/)).toBeInTheDocument();
  });
});

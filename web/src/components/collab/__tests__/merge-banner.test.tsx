import { describe, expect, it, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import { MergeBanner } from "../merge-banner";

const messages = {
  collab: {
    mergeBanner: {
      title: "{author} updated this entity",
      description:
        "Your unsaved edits sit on top of the remote update. Choose whether to rebase your changes, drop them in favour of the remote version, or compare side-by-side.",
      keepLocal: "Keep my edits",
      acceptRemote: "Take theirs",
      compare: "Compare",
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

describe("MergeBanner", () => {
  it("renders the author name in the title", () => {
    render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="Hyejin"
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(
      screen.getByText("Hyejin updated this entity"),
    ).toBeTruthy();
  });

  it("renders the optional change summary when supplied", () => {
    render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="Hyejin"
          remoteChangeSummary="Renamed `email` to `primary_email`"
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(
      screen.getByText("Renamed `email` to `primary_email`"),
    ).toBeTruthy();
  });

  it("Keep mine fires the local handler", () => {
    const onKeep = vi.fn();
    render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
          onKeepLocal={onKeep}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    fireEvent.click(screen.getByText("Keep my edits"));
    expect(onKeep).toHaveBeenCalledTimes(1);
  });

  it("Take theirs fires the remote handler", () => {
    const onAccept = vi.fn();
    render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
          onKeepLocal={() => {}}
          onAcceptRemote={onAccept}
        />
      </Wrapper>,
    );
    fireEvent.click(screen.getByText("Take theirs"));
    expect(onAccept).toHaveBeenCalledTimes(1);
  });

  it("Compare button only renders when onCompare supplied", () => {
    const onCompare = vi.fn();
    const { rerender } = render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    expect(screen.queryByText("Compare")).toBeNull();
    rerender(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
          onCompare={onCompare}
        />
      </Wrapper>,
    );
    fireEvent.click(screen.getByText("Compare"));
    expect(onCompare).toHaveBeenCalledTimes(1);
  });

  it("disables every action while busy", () => {
    const onKeep = vi.fn();
    render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
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

  it("`tone=info` swaps to info palette", () => {
    const { container } = render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
          tone="info"
        />
      </Wrapper>,
    );
    const root = container.firstElementChild;
    expect(root?.className).toContain("bg-info-surface");
    expect(root?.className).not.toContain("bg-warning-surface");
  });

  it("emits an aria-live polite region for screen readers", () => {
    const { container } = render(
      <Wrapper>
        <MergeBanner
          remoteAuthorName="A"
          onKeepLocal={() => {}}
          onAcceptRemote={() => {}}
        />
      </Wrapper>,
    );
    const root = container.firstElementChild;
    expect(root?.getAttribute("role")).toBe("alert");
    expect(root?.getAttribute("aria-live")).toBe("polite");
  });
});

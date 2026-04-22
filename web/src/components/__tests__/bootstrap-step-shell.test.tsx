import { afterEach, describe, it, expect, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import messages from "../../../messages/en.json";
import { StepShell } from "@/app/bootstrap/step-shell";
import { BootstrapProvider } from "@/app/bootstrap/bootstrap-state";

// Capture the mocked router so each test can assert navigation.
const push = vi.fn();
const back = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push, back }),
}));

function renderShell(overrides: Partial<Parameters<typeof StepShell>[0]> = {}) {
  push.mockClear();
  back.mockClear();
  const onFinish = vi.fn();
  render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <BootstrapProvider>
        <StepShell
          stepKey="1-pilot"
          nextPath="/bootstrap/2-source"
          canAdvance
          title="Scope the pilot"
          subtitle="Pick a narrow slice."
          onFinish={onFinish}
          {...overrides}
        >
          <div>body</div>
        </StepShell>
      </BootstrapProvider>
    </NextIntlClientProvider>,
  );
  return { onFinish };
}

afterEach(cleanup);

describe("Bootstrap StepShell", () => {
  it("renders title + subtitle + body + all nav buttons when nextPath is set", () => {
    renderShell();
    expect(screen.getByText("Scope the pilot")).toBeDefined();
    expect(screen.getByText("Pick a narrow slice.")).toBeDefined();
    expect(screen.getByText("body")).toBeDefined();
    expect(screen.getByRole("button", { name: /Back/ })).toBeDefined();
    expect(screen.getByRole("button", { name: /Skip/ })).toBeDefined();
    expect(screen.getByRole("button", { name: /^Next$/ })).toBeDefined();
  });

  it("Next routes to nextPath", () => {
    renderShell();
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    expect(push).toHaveBeenCalledWith("/bootstrap/2-source");
  });

  it("Skip routes to nextPath without marking complete", () => {
    renderShell();
    fireEvent.click(screen.getByRole("button", { name: /Skip/ }));
    expect(push).toHaveBeenCalledWith("/bootstrap/2-source");
  });

  it("Next is disabled when canAdvance is false", () => {
    renderShell({ canAdvance: false });
    const next = screen.getByRole("button", { name: /^Next$/ }) as HTMLButtonElement;
    expect(next.disabled).toBe(true);
  });

  it("renders Finish instead of Next when nextPath is null, and fires onFinish", () => {
    const { onFinish } = renderShell({ nextPath: null });
    const finish = screen.getByRole("button", { name: /Finish/ });
    expect(finish).toBeDefined();
    expect(screen.queryByRole("button", { name: /Skip/ })).toBeNull();
    fireEvent.click(finish);
    expect(onFinish).toHaveBeenCalled();
    expect(push).not.toHaveBeenCalled();
  });

  it("Back with an explicit backPath routes there", () => {
    renderShell({ backPath: "/bootstrap/1-pilot" });
    fireEvent.click(screen.getByRole("button", { name: /Back/ }));
    expect(push).toHaveBeenCalledWith("/bootstrap/1-pilot");
  });

  it("Back without backPath invokes router.back()", () => {
    renderShell({ backPath: undefined });
    fireEvent.click(screen.getByRole("button", { name: /Back/ }));
    expect(back).toHaveBeenCalled();
  });
});

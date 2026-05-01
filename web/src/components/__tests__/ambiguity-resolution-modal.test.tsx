import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import messages from "../../../messages/en.json";
import { ResolutionModal } from "@/components/ambiguity/resolution-modal";
import type { AmbiguityContext } from "@/lib/api/ambiguity";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function baseContext(): AmbiguityContext {
  return {
    id: "ctx-1",
    source_id: "src-postgres",
    column: { relation: "orders", column: "status" },
    kind: { kind: "numeric_code" },
    sample_values: ["1", "2", "3"],
    clarification_prompt: "What do these codes mean?",
    detection_source_hash: "sha256:abc",
    detected_at: "2026-04-22T00:00:00Z",
  };
}

function renderModal(props: Partial<Parameters<typeof ResolutionModal>[0]> = {}) {
  const onSubmit = vi.fn();
  const onCancel = vi.fn();
  render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <ResolutionModal
        context={baseContext()}
        active={null}
        onSubmit={onSubmit}
        onCancel={onCancel}
        {...props}
      />
    </NextIntlClientProvider>,
  );
  return { onSubmit, onCancel };
}

describe("ResolutionModal", () => {
  it("renders the clarification prompt and a pre-filled row per sample value", () => {
    renderModal();
    expect(screen.getByText("What do these codes mean?")).toBeDefined();
    // 3 sample values → 3 rows, each with the raw value input filled.
    expect((screen.getByLabelText(/Raw value row 1/) as HTMLInputElement).value).toBe("1");
    expect((screen.getByLabelText(/Raw value row 2/) as HTMLInputElement).value).toBe("2");
    expect((screen.getByLabelText(/Raw value row 3/) as HTMLInputElement).value).toBe("3");
  });

  it("Value Map submit filters out empty rows and emits an AmbiguityMapping", () => {
    const { onSubmit } = renderModal();

    const display1 = screen.getByLabelText(/Display label row 1/);
    fireEvent.change(display1, { target: { value: "Active" } });

    // Leave row 2 & 3 display empty — they should be filtered out.
    fireEvent.click(screen.getByRole("button", { name: /Save resolution/i }));

    expect(onSubmit).toHaveBeenCalledWith({
      kind: "value_map",
      entries: [{ value: "1", display: "Active", definition: null }],
    });
  });

  it("switching to CodeSystemRef mode + submitting emits code_system_ref", () => {
    const { onSubmit } = renderModal();
    // Click the Code system radio label.
    fireEvent.click(screen.getByRole("radio", { name: /Code system/i }));
    const input = screen.getByLabelText(/Code system id/);
    fireEvent.change(input, { target: { value: "cs-order-status" } });
    fireEvent.click(screen.getByRole("button", { name: /Save resolution/i }));
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "code_system_ref",
      code_system_id: "cs-order-status",
    });
  });

  it("pre-fills from an existing resolution's mapping", () => {
    const active = {
      id: "r-1",
      context_id: "ctx-1",
      context_source_hash: "sha256:abc",
      mapping: {
        kind: "value_map" as const,
        entries: [
          { value: "1", display: "OK" },
          { value: "2", display: "Failed", definition: "Terminal" },
        ],
      },
      resolved_at: "2026-04-22T00:00:00Z",
    };
    renderModal({ active });
    expect(
      (screen.getByLabelText(/Display label row 1/) as HTMLInputElement).value,
    ).toBe("OK");
    expect(
      (screen.getByLabelText(/Definition row 2/) as HTMLInputElement).value,
    ).toBe("Terminal");
  });

  it("Cancel fires onCancel without calling onSubmit", () => {
    const { onCancel, onSubmit } = renderModal();
    fireEvent.click(screen.getByRole("button", { name: /Cancel/i }));
    expect(onCancel).toHaveBeenCalled();
    expect(onSubmit).not.toHaveBeenCalled();
  });
});

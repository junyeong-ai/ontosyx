import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import messages from "../../../../messages/en.json";
import {
  ExistingPilotDialog,
  suggestRename,
} from "@/app/bootstrap/6-validate/existing-pilot-dialog";
import type { OntologyListItem } from "@/types/api";

function baseOntology(overrides: Partial<OntologyListItem> = {}): OntologyListItem {
  return {
    id: "ont-123",
    lineage_id: "lineage-abc",
    name: "Order pilot",
    description: { default: "", translations: {} },
    created_at: "2026-04-23T00:00:00Z",
    updated_at: "2026-04-23T00:00:00Z",
    ...overrides,
  };
}

function renderDialog(
  overrides: Partial<Parameters<typeof ExistingPilotDialog>[0]> = {},
) {
  const onChoose = vi.fn();
  render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <ExistingPilotDialog
        open
        existing={baseOntology()}
        renameSuggestion="Order pilot 2"
        onChoose={onChoose}
        {...overrides}
      />
    </NextIntlClientProvider>,
  );
  return { onChoose };
}

describe("suggestRename", () => {
  it("returns an empty string for whitespace-only input", () => {
    expect(suggestRename("")).toBe("");
    expect(suggestRename("   ")).toBe("");
  });

  it("appends ' 2' to names without a trailing integer", () => {
    expect(suggestRename("Order pilot")).toBe("Order pilot 2");
    expect(suggestRename("Alpha")).toBe("Alpha 2");
  });

  it("increments an existing trailing integer (walks forward, does not loop)", () => {
    expect(suggestRename("Order pilot 2")).toBe("Order pilot 3");
    expect(suggestRename("Pilot 99")).toBe("Pilot 100");
  });

  it("only treats a whitespace-separated trailing integer as a counter", () => {
    // `Pilot2` (no separator) stays as a name with no counter, so the
    // suggestion appends ' 2' rather than turning it into `Pilot3`.
    expect(suggestRename("Pilot2")).toBe("Pilot2 2");
  });
});

describe("ExistingPilotDialog", () => {
  it("renders title + description with the colliding pilot name + rename suggestion", () => {
    renderDialog();
    expect(
      screen.getByText(/A pilot named "Order pilot" already exists/i),
    ).toBeDefined();
    expect(screen.getByText(/start a new one as "Order pilot 2"/i)).toBeDefined();
  });

  it("Continue button emits 'continue'", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByTestId("existing-pilot-continue"));
    expect(onChoose).toHaveBeenCalledExactlyOnceWith("continue");
  });

  it("Rename button emits 'rename'", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByTestId("existing-pilot-rename"));
    expect(onChoose).toHaveBeenCalledExactlyOnceWith("rename");
  });

  it("Cancel button emits 'cancel'", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByTestId("existing-pilot-cancel"));
    expect(onChoose).toHaveBeenCalledExactlyOnceWith("cancel");
  });

  it("does not render content when open=false", () => {
    renderDialog({ open: false });
    expect(screen.queryByTestId("existing-pilot-dialog")).toBeNull();
  });
});

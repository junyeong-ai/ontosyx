import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import messages from "../../../messages/en.json";
import { PropertyEditor } from "@/components/workbench/inspector/property-editor";

const storeMock = vi.hoisted(() => ({
  applyCommand: vi.fn(),
}));

vi.mock("@/lib/store", () => ({
  useAppStore: (selector: (state: { applyCommand: typeof storeMock.applyCommand }) => unknown) =>
    selector({ applyCommand: storeMock.applyCommand }),
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("PropertyEditor", () => {
  beforeEach(() => {
    storeMock.applyCommand.mockReset();
  });

  it("emits the canonical date_time property type discriminant", () => {
    wrap(
      <PropertyEditor ownerId="node-customer" ownerKind="node" onClose={vi.fn()} />,
    );

    fireEvent.change(screen.getByPlaceholderText("Property name"), {
      target: { value: "created_at" },
    });
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "date_time" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(storeMock.applyCommand).toHaveBeenCalledWith(
      expect.objectContaining({
        op: "add_property",
        property: expect.objectContaining({
          name: "created_at",
          property_type: { type: "date_time" },
        }),
      }),
    );
  });
});

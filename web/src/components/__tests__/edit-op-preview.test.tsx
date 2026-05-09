import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";

import messages from "../../../messages/en.json";
import { EditOpPreview } from "@/components/settings/approvals/edit-op-preview";

function renderWithIntl(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("EditOpPreview", () => {
  it("renders concept CRUD operations with stable op labels and identifiers", () => {
    renderWithIntl(
      <EditOpPreview
        payload={{
          expected_version: 12,
          operations: [
            {
              op: "create_concept",
              def: { id: "c-customer", canonical_term_id: "gt-customer" },
            },
            {
              op: "update_concept",
              id: "c-customer",
              def: { id: "c-customer", canonical_term_id: "gt-customer" },
            },
            { op: "delete_concept", id: "c-obsolete" },
          ],
        }}
      />,
    );

    expect(screen.getByText("create_concept")).toBeInTheDocument();
    expect(screen.getByText("update_concept")).toBeInTheDocument();
    expect(screen.getByText("delete_concept")).toBeInTheDocument();
    expect(screen.getAllByText("c-customer")).toHaveLength(2);
    expect(screen.getByText("c-obsolete")).toBeInTheDocument();
    expect(screen.getByText(/v12/)).toBeInTheDocument();
  });
});

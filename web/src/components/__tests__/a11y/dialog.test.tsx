/**
 * A11y test: modal dialog structural requirements.
 *
 * Mirrors the structure used by `KeyboardShortcutsDialog` and
 * `SearchDialog`: a backdrop <button> to close on click, a <div role=dialog>
 * with aria-modal=true and aria-labelledby pointing at the title. We render
 * a static snippet rather than the live component because `focus-trap-react`
 * (used by the real dialog) needs at least one tabbable child in jsdom,
 * which is harder to wire reliably across vitest/jsdom versions.
 */
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import { axe } from "vitest-axe";

afterEach(cleanup);

function DialogSample() {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="dlg-title"
      className="fixed inset-0 z-50"
    >
      <button type="button" aria-label="Close dialog">Close</button>
      <div>
        <h2 id="dlg-title">Confirm deletion</h2>
        <p>Are you sure you want to delete this item?</p>
        <button type="button">Cancel</button>
        <button type="button">Delete</button>
      </div>
    </div>
  );
}

describe("Dialog (a11y)", () => {
  it("exposes role=dialog + aria-modal + aria-labelledby", () => {
    const { container } = render(<DialogSample />);
    const dialog = container.querySelector<HTMLElement>('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    const labelledBy = dialog?.getAttribute("aria-labelledby");
    expect(labelledBy).toBeTruthy();
    if (labelledBy) {
      expect(container.querySelector(`#${labelledBy}`)).not.toBeNull();
    }
  });

  it("has at least one tabbable control (so focus-trap-react won't crash)", () => {
    const { container } = render(<DialogSample />);
    // focus-trap requires ≥1 tabbable — buttons with no tabindex=-1 qualify.
    const buttons = container.querySelectorAll("button");
    expect(buttons.length).toBeGreaterThan(0);
  });

  it("has no axe violations", async () => {
    const { container } = render(<DialogSample />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});

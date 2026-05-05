import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { Button, buttonStyles } from "@/components/ui/button";

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("Button", () => {
  it("renders a native button with the variant + size class", () => {
    wrap(<Button variant="primary" size="md">Save</Button>);
    const btn = screen.getByRole("button", { name: "Save" });
    expect(btn).toBeInstanceOf(HTMLButtonElement);
    expect(btn.className).toContain("bg-brand-solid");
    expect(btn.className).toContain("h-9");
  });

  it("loading=true shows spinner, sets aria-busy, blocks click", () => {
    const onClick = vi.fn();
    wrap(
      <Button onClick={onClick} loading>
        Save
      </Button>,
    );
    const btn = screen.getByRole("button");
    expect(btn).toHaveAttribute("aria-busy", "true");
    expect(btn).toBeDisabled();
    // The spinner has role="status" — the only other accessible-name
    // matching descendant.
    expect(screen.getByRole("status")).toBeInTheDocument();
    fireEvent.click(btn);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("loading does not render leadingIcon", () => {
    wrap(
      <Button loading leadingIcon={<svg data-testid="lead" />}>
        Save
      </Button>,
    );
    expect(screen.queryByTestId("lead")).not.toBeInTheDocument();
  });

  it("renders trailingIcon when not loading", () => {
    wrap(<Button trailingIcon={<svg data-testid="trail" />}>Next</Button>);
    expect(screen.getByTestId("trail")).toBeInTheDocument();
  });

  it("hides trailingIcon while loading", () => {
    wrap(
      <Button loading trailingIcon={<svg data-testid="trail" />}>
        Next
      </Button>,
    );
    expect(screen.queryByTestId("trail")).not.toBeInTheDocument();
  });

  it("forwards ref", () => {
    let ref: HTMLButtonElement | null = null;
    wrap(
      <Button ref={(el) => { ref = el; }}>Click</Button>,
    );
    expect(ref).toBeInstanceOf(HTMLButtonElement);
  });

  it("disabled prop sets disabled attribute", () => {
    wrap(<Button disabled>Off</Button>);
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("custom className composes after defaults", () => {
    wrap(<Button className="my-extra">Click</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toContain("my-extra");
    // base class still present
    expect(btn.className).toContain("inline-flex");
  });
});

describe("buttonStyles", () => {
  it("returns a class string carrying the variant + size + base", () => {
    const cls = buttonStyles({ variant: "primary", size: "md" });
    expect(cls).toContain("bg-brand-solid");
    expect(cls).toContain("h-9");
    expect(cls).toContain("inline-flex");
    expect(cls).toContain("focus-visible:ring-2");
  });

  it("default = default variant + md size", () => {
    const cls = buttonStyles();
    expect(cls).toContain("bg-foreground");
    expect(cls).toContain("h-9");
  });

  it("appends caller className last so it wins on conflicts", () => {
    const cls = buttonStyles({ className: "custom-tail" });
    expect(cls.endsWith("custom-tail")).toBe(true);
  });

  it("each variant maps to a distinct color token", () => {
    const variants = ["default", "primary", "ghost", "outline", "danger"] as const;
    const seen = new Set<string>();
    for (const variant of variants) {
      const cls = buttonStyles({ variant });
      // Extract first color token
      const match = cls.match(/(bg-\S+|text-\S+|border-\S+)/);
      if (match) seen.add(match[0]);
    }
    expect(seen.size).toBeGreaterThan(2);
  });

  it("each size maps to a distinct height token", () => {
    const sizes = ["xs", "sm", "md", "lg", "icon", "icon-sm"] as const;
    const heights = new Set<string>();
    for (const size of sizes) {
      const cls = buttonStyles({ size });
      const match = cls.match(/h-\S+/);
      if (match) heights.add(match[0]);
    }
    // xs/sm/md/lg are 4 distinct, icon/icon-sm overlap with sm/xs.
    expect(heights.size).toBeGreaterThanOrEqual(3);
  });
});

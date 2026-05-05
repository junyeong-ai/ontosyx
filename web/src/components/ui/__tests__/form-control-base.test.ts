import { describe, it, expect } from "vitest";

import { formControlBase } from "@/components/ui/form-input";

describe("formControlBase", () => {
  it("returns the shared border + focus + aria-invalid contract", () => {
    const cls = formControlBase("default");
    expect(cls).toContain("border-divider");
    expect(cls).toContain("focus:border-brand-foreground");
    expect(cls).toContain("focus:ring-1");
    expect(cls).toContain("aria-invalid:border-danger-border");
    expect(cls).toContain("disabled:cursor-not-allowed");
    expect(cls).toContain("placeholder:text-foreground-subtle");
  });

  it("default density is 'default' when called without args", () => {
    const explicit = formControlBase("default");
    const implicit = formControlBase();
    expect(implicit).toBe(explicit);
  });

  it("each density carries a distinct padding + text-size", () => {
    const expectations: Record<string, [RegExp, RegExp]> = {
      default: [/\bpx-3\b/, /\btext-sm\b/],
      settings: [/\bpx-3\b/, /\btext-xs\b/],
      compact: [/\bpx-2\b/, /\btext-2xs\b/],
    };
    for (const [density, [pxRe, txtRe]] of Object.entries(expectations)) {
      const cls = formControlBase(density as Parameters<typeof formControlBase>[0]);
      expect(cls).toMatch(pxRe);
      expect(cls).toMatch(txtRe);
    }
  });

  it("densities differ", () => {
    expect(formControlBase("default")).not.toBe(formControlBase("compact"));
    expect(formControlBase("settings")).not.toBe(formControlBase("compact"));
    expect(formControlBase("default")).not.toBe(formControlBase("settings"));
  });
});

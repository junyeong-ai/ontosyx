import { describe, it, expect } from "vitest";

import {
  auditCalls,
  findTranslationCalls,
  reasonLabel,
} from "../lib/i18n-auditor.mjs";

// Tiny deterministic bundles used to exercise each finding shape.
// `en` and `ko` diverge in a few places so the scanner can report
// "missing in en" / "missing in ko" / "missing in both" separately.
const EN = {
  settings: {
    quality: {
      title: "Quality",
      toast: { createFailed: "Failed to create rule" },
      kind: { critical: "Critical", warning: "Warning" },
    },
  },
};

const KO = {
  settings: {
    quality: {
      title: "품질",
      toast: {},
      kind: { critical: "치명", warning: "경고" },
    },
  },
};

describe("findTranslationCalls — AST-based scanner", () => {
  it("attaches every call to the enclosing useTranslations namespace", () => {
    const src = `
      import { useTranslations } from "next-intl";
      function A() {
        const t = useTranslations("settings.quality");
        return <div>{t("title")}</div>;
      }
    `;
    const calls = findTranslationCalls("virtual/A.tsx", src);
    expect(calls).toHaveLength(1);
    expect(calls[0].namespace).toBe("settings.quality");
    expect(calls[0].ref).toEqual({ kind: "static", key: "title" });
  });

  it("gives each sub-component its own lexical scope", () => {
    // Two `const t` declarations in the same file must NOT alias
    // each other — the inner function's `t` is distinct from the
    // outer one even though they share the name.
    const src = `
      import { useTranslations } from "next-intl";
      function Outer() {
        const t = useTranslations("settings.quality");
        return <div>{t("title")}</div>;
      }
      function Inner() {
        const t = useTranslations("settings.quality.toast");
        return <div>{t("createFailed")}</div>;
      }
    `;
    const calls = findTranslationCalls("virtual/Pair.tsx", src);
    expect(calls).toHaveLength(2);
    const titleCall = calls.find((c) => c.ref.kind === "static" && c.ref.key === "title");
    const toastCall = calls.find(
      (c) => c.ref.kind === "static" && c.ref.key === "createFailed",
    );
    expect(titleCall?.namespace).toBe("settings.quality");
    expect(toastCall?.namespace).toBe("settings.quality.toast");
  });

  it("treats `t.rich(...)` like `t(...)` for key extraction", () => {
    const src = `
      import { useTranslations } from "next-intl";
      function C() {
        const t = useTranslations("settings.quality");
        return <>{t.rich("title")}</>;
      }
    `;
    const [call] = findTranslationCalls("virtual/Rich.tsx", src);
    expect(call.ref).toEqual({ kind: "static", key: "title" });
  });

  it("extracts the static head of a template literal as a prefix", () => {
    const src = `
      import { useTranslations } from "next-intl";
      function D({ kind }: { kind: string }) {
        const t = useTranslations("settings.quality");
        return <>{t(\`kind.\${kind}\`)}</>;
      }
    `;
    const [call] = findTranslationCalls("virtual/D.tsx", src);
    // Head "kind." → trailing-dot trimmed → "kind"
    expect(call.ref).toEqual({ kind: "prefix", prefix: "kind" });
  });

  it("falls back to the parent object when the template glues onto a partial word", () => {
    // `readOnly.reason${x}` → UI glues Rust variant names directly
    // onto `reason` with no dot (`reasonMatch`, `reasonPathFind`).
    // The static part that CAN be verified is the parent path
    // before the last dot — here, `readOnly`.
    const src = `
      import { useTranslations } from "next-intl";
      function E({ reason }: { reason: string }) {
        const t = useTranslations("settings.quality");
        return <>{t(\`readOnly.reason\${reason}\`)}</>;
      }
    `;
    const [call] = findTranslationCalls("virtual/E.tsx", src);
    expect(call.ref).toEqual({ kind: "prefix", prefix: "readOnly" });
  });

  it("skips template literals with no static dot (nothing verifiable)", () => {
    const src = `
      import { useTranslations } from "next-intl";
      function F({ x }: { x: string }) {
        const t = useTranslations("settings.quality");
        return <>{t(\`\${x}\`)}</>;
      }
    `;
    const calls = findTranslationCalls("virtual/F.tsx", src);
    expect(calls).toHaveLength(0);
  });

  it("skips calls with non-literal arguments (e.g. t(variable))", () => {
    const src = `
      import { useTranslations } from "next-intl";
      function G({ key }: { key: string }) {
        const t = useTranslations("settings.quality");
        return <>{t(key)}</>;
      }
    `;
    const calls = findTranslationCalls("virtual/G.tsx", src);
    expect(calls).toHaveLength(0);
  });
});

describe("auditCalls — bundle comparison", () => {
  it("returns no findings when every call resolves cleanly", () => {
    const calls = findTranslationCalls(
      "virtual/Clean.tsx",
      `
        import { useTranslations } from "next-intl";
        function C() {
          const t = useTranslations("settings.quality");
          return <div>{t("title")}</div>;
        }
      `,
    );
    const findings = auditCalls(calls, { en: EN, ko: KO });
    expect(findings).toEqual([]);
  });

  it("flags a key that exists in en but not ko", () => {
    const calls = findTranslationCalls(
      "virtual/Missing.tsx",
      `
        import { useTranslations } from "next-intl";
        function M() {
          const t = useTranslations("settings.quality.toast");
          return <div>{t("createFailed")}</div>;
        }
      `,
    );
    const findings = auditCalls(calls, { en: EN, ko: KO });
    expect(findings).toHaveLength(1);
    expect(findings[0].path).toBe("settings.quality.toast.createFailed");
    expect(findings[0].reason).toBe("missing_in_ko");
  });

  it("flags a prefix that resolves to a leaf in either bundle", () => {
    // Simulate the bug we found during the i18n sweep: the bundle
    // has `settings.quality.title` as a string, but some file
    // accidentally calls `t(\`title.\${x}\`)` expecting it to be
    // an object. Both bundles have `title` as a string, so the
    // prefix check fails.
    const calls = findTranslationCalls(
      "virtual/Leaf.tsx",
      `
        import { useTranslations } from "next-intl";
        function L({ k }: { k: string }) {
          const t = useTranslations("settings.quality");
          return <>{t(\`title.\${k}\`)}</>;
        }
      `,
    );
    const findings = auditCalls(calls, { en: EN, ko: KO });
    expect(findings).toHaveLength(1);
    expect(findings[0].reason).toBe("prefix_is_leaf");
    expect(findings[0].path).toBe("settings.quality.title");
  });

  it("flags a key missing in both bundles", () => {
    const calls = findTranslationCalls(
      "virtual/Both.tsx",
      `
        import { useTranslations } from "next-intl";
        function B() {
          const t = useTranslations("settings.quality");
          return <>{t("does.not.exist")}</>;
        }
      `,
    );
    const findings = auditCalls(calls, { en: EN, ko: KO });
    expect(findings[0].reason).toBe("missing_in_both");
  });
});

describe("reasonLabel", () => {
  it("maps every reason to a stable string", () => {
    expect(reasonLabel("missing_in_en")).toBe("missing in en");
    expect(reasonLabel("missing_in_ko")).toBe("missing in ko");
    expect(reasonLabel("missing_in_both")).toBe("missing in both");
    expect(reasonLabel("prefix_is_leaf")).toContain("leaf");
  });
});

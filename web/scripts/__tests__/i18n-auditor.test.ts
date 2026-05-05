import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type ts from "typescript";
import { afterAll, beforeAll, describe, it, expect } from "vitest";

import {
  auditCalls,
  createAuditProgram,
  findHardcodedJsxStrings,
  findTranslationCalls,
  findTranslationCallsTyped,
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

// ---------------------------------------------------------------------------
// Typed-scan tests — these spin up a real TypeScript Program so the
// type checker is live. We write sources to a tempdir + tsconfig,
// compile, and walk. The fixture is reused across the suite so the
// Program-build cost (~hundreds of ms) is paid once.
// ---------------------------------------------------------------------------

describe("findTranslationCallsTyped — enum-resolved templates", () => {
  let tmpRoot: string;
  let program: ts.Program;

  /**
   * Write a file into the temp workspace and return its absolute
   * path. Caller is free to pass relative slashes; the function
   * normalises against `tmpRoot`.
   */
  function writeFile(rel: string, contents: string): string {
    const abs = path.join(tmpRoot, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, contents, "utf8");
    return abs;
  }

  beforeAll(() => {
    tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "i18n-audit-"));

    writeFile(
      "tsconfig.json",
      JSON.stringify({
        compilerOptions: {
          target: "ES2020",
          module: "ESNext",
          moduleResolution: "Bundler",
          jsx: "react-jsx",
          strict: true,
          noEmit: true,
          skipLibCheck: true,
        },
        include: ["**/*.ts", "**/*.tsx"],
      }),
    );

    // Stub just enough of next-intl's surface that the type checker
    // doesn't complain about the `useTranslations` import.
    writeFile(
      "next-intl.d.ts",
      `declare module "next-intl" {
         export function useTranslations(ns: string): (key: string) => string;
       }`,
    );

    // The real subject under test — the exact pattern that the
    // parse-only auditor missed until we added type-checker support.
    writeFile(
      "EnumTemplate.tsx",
      `
        import { useTranslations } from "next-intl";
        const REASONS = ["Match", "PathFind", "Aggregate"] as const;
        type Reason = (typeof REASONS)[number];
        function isReason(s: string): s is Reason {
          return (REASONS as readonly string[]).includes(s);
        }
        export function Banner({ r }: { r: string }) {
          const t = useTranslations("ns");
          if (!isReason(r)) return null;
          return <span>{t(\`reason.\${r}\`)}</span>;
        }
      `,
    );

    // Control: a literal template whose expression widens to plain
    // string. The typed scan must NOT claim an enum here — it falls
    // back to the parent-path form so only "prefix" is checked.
    writeFile(
      "PlainTemplate.tsx",
      `
        import { useTranslations } from "next-intl";
        export function Cell({ raw }: { raw: string }) {
          const t = useTranslations("ns");
          return <span>{t(\`reason.\${raw}\`)}</span>;
        }
      `,
    );

    program = createAuditProgram(path.join(tmpRoot, "tsconfig.json"));
  });

  afterAll(() => {
    fs.rmSync(tmpRoot, { recursive: true, force: true });
  });

  it("enumerates `as const` arrays into concrete enum_prefix values", () => {
    const [call] = findTranslationCallsTyped(
      path.join(tmpRoot, "EnumTemplate.tsx"),
      program,
    );
    expect(call.ref).toEqual({
      kind: "enum_prefix",
      prefix: "reason.",
      values: ["Match", "PathFind", "Aggregate"],
    });
  });

  it("falls back to `prefix` when the template expression is plain string", () => {
    const [call] = findTranslationCallsTyped(
      path.join(tmpRoot, "PlainTemplate.tsx"),
      program,
    );
    expect(call.ref).toEqual({ kind: "prefix", prefix: "reason" });
  });

  it("auditCalls reports one finding per missing concrete leaf", () => {
    const calls = findTranslationCallsTyped(
      path.join(tmpRoot, "EnumTemplate.tsx"),
      program,
    );
    // Bundle has Match + Aggregate but NOT PathFind.
    const en = {
      ns: { reason: { Match: "매치", Aggregate: "집계" } },
    };
    const ko = en;
    const findings = auditCalls(calls, { en, ko });
    expect(findings).toHaveLength(1);
    expect(findings[0].path).toBe("ns.reason.PathFind");
    expect(findings[0].reason).toBe("missing_in_both");
  });
});

// ---------------------------------------------------------------------------
// findHardcodedJsxStrings
// ---------------------------------------------------------------------------

describe("findHardcodedJsxStrings", () => {
  function scan(source: string): ReturnType<typeof findHardcodedJsxStrings> {
    return findHardcodedJsxStrings("/virtual/file.tsx", source);
  }

  it("flags multi-word prose on placeholder", () => {
    const out = scan(`
      export function X() {
        return <input placeholder="Search graph entities" />;
      }
    `);
    expect(out).toHaveLength(1);
    expect(out[0].attribute).toBe("placeholder");
    expect(out[0].value).toBe("Search graph entities");
  });

  it("flags single-word ≥4-char prose ('Save', 'Edit', 'Done')", () => {
    const out = scan(`
      export function X() {
        return (
          <>
            <button aria-label="Save" />
            <button aria-label="Edit" />
            <button aria-label="Done" />
          </>
        );
      }
    `);
    expect(out.map((f) => f.value).sort()).toEqual(["Done", "Edit", "Save"]);
  });

  it("ignores ≤4-char all-uppercase acronyms ('API', 'URL', 'CSV')", () => {
    const out = scan(`
      export function X() {
        return (
          <>
            <a aria-label="API" />
            <a aria-label="URL" />
            <a aria-label="CSV" />
          </>
        );
      }
    `);
    expect(out).toEqual([]);
  });

  it("ignores presentation glyphs ('•', '→', '---')", () => {
    const out = scan(`
      export function X() {
        return (
          <>
            <span title="•" />
            <span title="→" />
            <span title="---" />
          </>
        );
      }
    `);
    expect(out).toEqual([]);
  });

  it("flags hyphen / underscore identifiers when they contain ≥4 alpha chars — marker opts them out", () => {
    // `cs-order-status` and `tenant_id` both contain a 4+ alpha run
    // ("order" / "status" / "tenant"), so they trip the heuristic.
    // Real intent (these are language-neutral slugs) is communicated
    // through the explicit `// i18n-audit-ignore` marker; the
    // heuristic alone isn't smart enough to know.
    const flagged = scan(`
      export function X() {
        return (
          <>
            <input placeholder="cs-order-status" />
            <input placeholder="tenant_id" />
          </>
        );
      }
    `);
    expect(flagged.map((f) => f.value).sort()).toEqual([
      "cs-order-status",
      "tenant_id",
    ]);
    const ignored = scan(`
      export function X() {
        return (
          <>
            {/* i18n-audit-ignore — slug */}
            <input placeholder="cs-order-status" />
            {/* i18n-audit-ignore — column id */}
            <input placeholder="tenant_id" />
          </>
        );
      }
    `);
    expect(ignored).toEqual([]);
  });

  it("ignores expression attribute values ({t('foo')}) entirely — only flags string literals", () => {
    const out = scan(`
      export function X({ t }: { t: (k: string) => string }) {
        return <input placeholder={t("searchPlaceholder")} />;
      }
    `);
    expect(out).toEqual([]);
  });

  it("only checks the four a11y-critical attributes — ignores className, value, etc.", () => {
    const out = scan(`
      export function X() {
        return <input className="text-sm" value="Save changes" data-testid="Search graph" />;
      }
    `);
    expect(out).toEqual([]);
  });

  it("respects the // i18n-audit-ignore line marker for the next 2 lines", () => {
    const out = scan(`
      export function X() {
        return (
          <input
            // i18n-audit-ignore — slug example
            placeholder="rule-min-email"
          />
        );
      }
    `);
    // The placeholder is on the line after the comment; the marker
    // suppresses lines [comment, comment+1, comment+2].
    expect(out).toEqual([]);
  });

  it("does NOT suppress findings outside the marker's window", () => {
    // The marker covers itself + the next 2 lines (intentional —
    // multi-line `<Component\n  prop="value"\n/>` patterns are
    // common). Anything farther down requires its own marker.
    const out = scan(`
      export function X() {
        return (
          <>
            {/* i18n-audit-ignore */}
            <input placeholder="ignored slug" />
            <hr />
            <hr />
            <input placeholder="far below the marker" />
          </>
        );
      }
    `);
    expect(out).toHaveLength(1);
    expect(out[0].value).toBe("far below the marker");
  });

  it("flags JSX-expression-wrapped string literals — attr={\"literal\"} too", () => {
    const out = scan(`
      export function X() {
        return <input placeholder={"Search graph entities"} />;
      }
    `);
    expect(out).toHaveLength(1);
    expect(out[0].value).toBe("Search graph entities");
  });
});

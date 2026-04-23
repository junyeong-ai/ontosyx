// i18n auditor — TypeScript AST–based scanner for missing translation
// keys in next-intl bundles.
//
// WHY AN AST WALKER, NOT REGEX:
//   A file can hold multiple `const t = useTranslations("<ns>")`
//   declarations (one per sub-component), each scoped to its own
//   function body. Regex can't tell which `t(...)` call resolves
//   against which namespace. The compiler's own parser handles
//   lexical scope precisely — the scanner walks the AST and threads
//   a scope chain so every call resolves to the innermost matching
//   declaration.
//
// SUPPORTED CALL FORMS:
//   - `t("foo.bar")`        → `<ns>.foo.bar` (static; checked exactly)
//   - `t.rich("foo.bar")`   → same as above
//   - `t(`kinds.${x}`)`     → `<ns>.kinds` must exist AND be an object
//                             (dynamic; checked as a prefix)
//   - `t(variable)`         → skipped (not statically analysable)
//
// PUBLIC API:
//   - `findTranslationCalls(filePath, source)` — extract every
//     statically-resolvable call as a `TranslationCall` record
//   - `auditCalls(calls, bundles)` — compare each call against the
//     bundles and return a list of `Finding`s (one per gap)
//   - `loadBundle(path)` — convenience reader that parses JSON and
//     surfaces parse errors with the file name baked in
//   - `walkSource(rootDir)` — yields the tsx/ts files the auditor
//     should scan (skips node_modules, .next, __tests__)
//
// The CLI entrypoint at `web/scripts/i18n-audit.mjs` composes these
// primitives and prints the result; the same API is unit-tested by
// `web/scripts/__tests__/i18n-auditor.test.ts`.

import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

/**
 * @typedef {{ kind: "static", key: string } |
 *           { kind: "prefix", prefix: string }} KeyRef
 * A statically-analysable key reference. `prefix` form covers
 * template literals like `` `kinds.${x}` `` — we can only verify the
 * head exists as an object, not that every dynamic suffix resolves.
 */

/**
 * @typedef {{
 *   file: string,
 *   line: number,
 *   namespace: string,
 *   ref: KeyRef,
 * }} TranslationCall
 */

/** @typedef {Record<string, unknown>} Bundle */

/**
 * @typedef {{
 *   file: string,
 *   line: number,
 *   path: string,
 *   reason:
 *     | "missing_in_en"
 *     | "missing_in_ko"
 *     | "missing_in_both"
 *     | "prefix_is_leaf",
 * }} Finding
 */

// ---------------------------------------------------------------------------
// AST scan
// ---------------------------------------------------------------------------

/**
 * Recursively walks a source file and collects every statically
 * resolvable `t("key")`, `t.rich("key")`, or `t(\`prefix.${x}\`)`
 * call, tagged with the `useTranslations` namespace active at the
 * call's lexical position.
 *
 * @param {string} filePath
 * @param {string} source
 * @returns {TranslationCall[]}
 */
export function findTranslationCalls(filePath, source) {
  const sourceFile = ts.createSourceFile(
    filePath,
    source,
    ts.ScriptTarget.Latest,
    /* setParentNodes */ true,
    ts.ScriptKind.TSX,
  );

  /** @type {TranslationCall[]} */
  const out = [];

  /**
   * A lexical scope chain. Each function body pushes a new frame;
   * lookups walk up the parent pointers. That's enough to model
   * next-intl's usage pattern where each sub-component declares its
   * own `const t = useTranslations(...)`.
   *
   * @typedef {{ aliases: Map<string, string>, parent: Scope | null }} Scope
   * @type {Scope}
   */
  const rootScope = { aliases: new Map(), parent: null };

  /** @param {Scope} scope @param {string} name @returns {string | undefined} */
  function lookup(scope, name) {
    if (scope.aliases.has(name)) return scope.aliases.get(name);
    return scope.parent ? lookup(scope.parent, name) : undefined;
  }

  /**
   * @param {ts.Node} node
   * @param {Scope} scope
   */
  function visit(node, scope) {
    // `const/let/var <alias> = useTranslations("<ns>")` declarations
    // register an alias in the enclosing scope. The declaration can
    // live anywhere inside a function body — hoisting is not an
    // issue since next-intl use sites always follow the declaration.
    if (ts.isVariableDeclaration(node)) {
      const alias = extractUseTranslationsAlias(node);
      if (alias) scope.aliases.set(alias.alias, alias.namespace);
    }

    // Call expression: `t("key")` | `t.rich("key")` | `t(\`p.${x}\`)`.
    // The call can appear before any declaration is registered in
    // this scope — that's an authoring bug (ReferenceError at
    // runtime) which we let the type checker catch, so it's fine
    // to ignore here.
    if (ts.isCallExpression(node)) {
      const target = callTarget(node.expression);
      if (target) {
        const namespace = lookup(scope, target);
        if (namespace !== undefined && node.arguments.length > 0) {
          const ref = extractKeyRef(node.arguments[0]);
          if (ref) {
            const { line } = sourceFile.getLineAndCharacterOfPosition(
              node.getStart(sourceFile),
            );
            out.push({
              file: filePath,
              line: line + 1,
              namespace,
              ref,
            });
          }
        }
      }
    }

    // Function-like nodes push a fresh scope frame. Block-scoped
    // `if`/`for`/etc. don't introduce a new `t` namespace binding
    // for our purposes (useTranslations is a React hook and only
    // legal at the top of a function body), so we don't model
    // block scopes.
    const opensScope =
      ts.isFunctionDeclaration(node) ||
      ts.isFunctionExpression(node) ||
      ts.isArrowFunction(node) ||
      ts.isMethodDeclaration(node) ||
      ts.isConstructorDeclaration(node) ||
      ts.isGetAccessorDeclaration(node) ||
      ts.isSetAccessorDeclaration(node);

    if (opensScope) {
      const child = { aliases: new Map(), parent: scope };
      ts.forEachChild(node, (c) => visit(c, child));
    } else {
      ts.forEachChild(node, (c) => visit(c, scope));
    }
  }

  visit(sourceFile, rootScope);
  return out;
}

/**
 * Pull the `{ alias, namespace }` pair out of a `const t =
 * useTranslations("ns")` style declaration. Returns `null` for any
 * other shape (destructuring, non-literal argument, etc.).
 *
 * @param {ts.VariableDeclaration} decl
 * @returns {{ alias: string, namespace: string } | null}
 */
function extractUseTranslationsAlias(decl) {
  if (!ts.isIdentifier(decl.name)) return null;
  const init = decl.initializer;
  if (!init || !ts.isCallExpression(init)) return null;
  if (!ts.isIdentifier(init.expression)) return null;
  if (init.expression.text !== "useTranslations") return null;
  if (init.arguments.length === 0) return null;
  const arg = init.arguments[0];
  if (!ts.isStringLiteralLike(arg)) return null;
  return { alias: decl.name.text, namespace: arg.text };
}

/**
 * Return the alias name for `<alias>(...)` or `<alias>.rich(...)`.
 * `null` for any other call form (method chains, computed access,
 * call on something that isn't a bare identifier, etc.).
 *
 * @param {ts.Expression} expr
 * @returns {string | null}
 */
function callTarget(expr) {
  if (ts.isIdentifier(expr)) return expr.text;
  if (
    ts.isPropertyAccessExpression(expr) &&
    ts.isIdentifier(expr.expression) &&
    ts.isIdentifier(expr.name) &&
    expr.name.text === "rich"
  ) {
    return expr.expression.text;
  }
  return null;
}

/**
 * Decide what piece of the bundle a call argument points at. String
 * literals become exact keys; template literals become a prefix
 * that must at least exist as an object. Anything else — variables,
 * conditional expressions — is skipped (we can't check it
 * statically without the type checker).
 *
 * @param {ts.Expression} arg
 * @returns {KeyRef | null}
 */
function extractKeyRef(arg) {
  if (ts.isStringLiteralLike(arg)) {
    return { kind: "static", key: arg.text };
  }
  if (ts.isTemplateExpression(arg)) {
    // `` `head${expr}...` `` — only the raw `head` is stable. What
    // we can verify depends on where the last `.` sits:
    //
    //   `kind.${x}`           → head = "kind."
    //     → verify `bundle.kind` is an object (standard prefix form)
    //   `kinds.sub.${x}`      → head = "kinds.sub."
    //     → verify `bundle.kinds.sub` is an object
    //   `readOnly.reason${x}` → head = "readOnly.reason"
    //     → the dynamic part glues onto `reason` (no dot), so the
    //       *parent* object `readOnly` is all we can verify; the
    //       actual leaf is `readOnly.reason<Match|PathFind|…>`
    //   `prefix${x}`          → head = "prefix"
    //     → parent is the namespace root (always exists) → skip
    //
    // Collapsing both cases: take everything before the last `.`.
    // If there's no `.`, no static verification is possible.
    const head = arg.head.text;
    const lastDot = head.lastIndexOf(".");
    if (lastDot === -1) return null;
    const prefix = head.slice(0, lastDot);
    if (prefix.length === 0) return null;
    return { kind: "prefix", prefix };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Bundle resolution
// ---------------------------------------------------------------------------

/**
 * @param {string} file
 * @returns {Bundle}
 */
export function loadBundle(file) {
  const raw = fs.readFileSync(file, "utf8");
  try {
    return JSON.parse(raw);
  } catch (err) {
    const cause = err instanceof Error ? err.message : String(err);
    throw new Error(`Failed to parse i18n bundle ${file}: ${cause}`);
  }
}

/**
 * Resolve a dotted path against a bundle.
 *
 * @param {Bundle} bundle
 * @param {string} dotted
 * @returns {unknown}
 */
function resolvePath(bundle, dotted) {
  /** @type {unknown} */
  let cur = bundle;
  for (const segment of dotted.split(".")) {
    if (cur && typeof cur === "object" && segment in /** @type {Record<string, unknown>} */ (cur)) {
      cur = /** @type {Record<string, unknown>} */ (cur)[segment];
    } else {
      return undefined;
    }
  }
  return cur;
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

/**
 * Compare every translation call against every provided bundle and
 * return one finding per gap. Findings are stable under re-ordering
 * of calls (grouped per call, not per bundle).
 *
 * @param {TranslationCall[]} calls
 * @param {{ en: Bundle, ko: Bundle }} bundles
 * @returns {Finding[]}
 */
export function auditCalls(calls, bundles) {
  /** @type {Finding[]} */
  const findings = [];

  for (const call of calls) {
    // Build the full dotted path to inspect in each bundle. The
    // ref's prefix / key is already normalised by `extractKeyRef`
    // (no trailing dot, no leading dot), so this is a plain join.
    const tail =
      call.ref.kind === "static" ? call.ref.key : call.ref.prefix;
    const fullPath = tail ? `${call.namespace}.${tail}` : call.namespace;

    const enHit = resolvePath(bundles.en, fullPath);
    const koHit = resolvePath(bundles.ko, fullPath);

    const missingEn = enHit === undefined;
    const missingKo = koHit === undefined;

    if (missingEn || missingKo) {
      findings.push({
        file: call.file,
        line: call.line,
        path: fullPath,
        reason:
          missingEn && missingKo
            ? "missing_in_both"
            : missingEn
              ? "missing_in_en"
              : "missing_in_ko",
      });
      continue;
    }

    // For prefix references the bundle hit must be an object — a
    // leaf value means the code tries to concatenate dynamic
    // suffixes onto a string, which is always a bug.
    if (call.ref.kind === "prefix") {
      const enIsObject =
        enHit !== null && typeof enHit === "object" && !Array.isArray(enHit);
      const koIsObject =
        koHit !== null && typeof koHit === "object" && !Array.isArray(koHit);
      if (!enIsObject || !koIsObject) {
        findings.push({
          file: call.file,
          line: call.line,
          path: fullPath,
          reason: "prefix_is_leaf",
        });
      }
    }
  }

  return findings;
}

// ---------------------------------------------------------------------------
// File walker
// ---------------------------------------------------------------------------

/**
 * Enumerate source files the auditor should scan. Skips
 * `node_modules`, `.next`, test files, and type declaration files —
 * none of those should carry i18n calls at runtime.
 *
 * @param {string} rootDir
 * @returns {string[]}
 */
export function walkSource(rootDir) {
  /** @type {string[]} */
  const out = [];
  /** @param {string} dir */
  const recurse = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === "node_modules" || entry.name === ".next") continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        recurse(full);
      } else if (
        /\.(tsx?|jsx?)$/.test(entry.name) &&
        !entry.name.endsWith(".d.ts") &&
        !full.includes("__tests__")
      ) {
        out.push(full);
      }
    }
  };
  recurse(rootDir);
  return out;
}

/**
 * Human-readable label for a finding reason. Kept in sync with the
 * CLI output so tests can assert on it.
 *
 * @param {Finding["reason"]} reason
 * @returns {string}
 */
export function reasonLabel(reason) {
  switch (reason) {
    case "missing_in_en":
      return "missing in en";
    case "missing_in_ko":
      return "missing in ko";
    case "missing_in_both":
      return "missing in both";
    case "prefix_is_leaf":
      return "prefix resolves to a leaf value (expected object)";
  }
}

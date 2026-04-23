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
// TYPE-CHECKED ENUM RESOLUTION:
//   When a TypeScript `Program` is passed in, the scanner promotes
//   `prefix` calls to `enum_prefix` whenever the template expression's
//   type resolves to a union of string literals — e.g.
//   `t(\`reason${x}\`)` where `x: "Match" | "PathFind" | ...`
//   emits `{ kind: "enum_prefix", prefix: "reason", values: [...] }`
//   and `auditCalls` checks every concrete `prefix<value>` combo
//   against the bundle. This catches the "I added a new enum
//   variant but forgot to add the matching i18n key" regression
//   class that the bare prefix check misses.
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
 *           { kind: "prefix", prefix: string } |
 *           { kind: "enum_prefix", prefix: string, values: string[] }} KeyRef
 * A statically-analysable key reference.
 *
 * - `static`: literal key (`"foo.bar"`) — checked exactly.
 * - `prefix`: template with an unresolved expression — we only
 *   verify the head exists as an object in the bundle.
 * - `enum_prefix`: template whose expression's TypeScript type is a
 *   finite union of string literals; we check every concatenated
 *   `<prefix><value>` (no separator, matches how the UI glues the
 *   dynamic suffix onto the head). Populated by `scanWithProgram`
 *   when a type-checker-backed Program is provided.
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
 * Parse-only scan. Walks a file's AST in isolation and yields every
 * statically resolvable `t(...)` / `t.rich(...)` call tagged with
 * the `useTranslations` namespace active at its lexical position.
 * Template literals with variable interpolation surface as bare
 * `prefix` refs — no cross-file type info is available.
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
  return walkSourceFile(sourceFile, filePath, null);
}

/**
 * Type-checked scan. Pulls the SourceFile out of a pre-built
 * `Program` so cross-file type information is live, and resolves
 * every template-literal expression through the type checker. When
 * the expression's type narrows to a finite union of string
 * literals (the `as const` array / `isKnown*` guard pattern), the
 * call gets promoted to `enum_prefix` and each concrete leaf is
 * checked individually.
 *
 * Falls back silently when the Program has no entry for `filePath`
 * — e.g. paths excluded by the tsconfig `include` glob.
 *
 * @param {string} filePath
 * @param {ts.Program} program
 * @returns {TranslationCall[]}
 */
export function findTranslationCallsTyped(filePath, program) {
  const sourceFile = program.getSourceFile(filePath);
  if (!sourceFile) return [];
  return walkSourceFile(sourceFile, filePath, program.getTypeChecker());
}

/**
 * Shared walker — the only bit that knows how to traverse an AST,
 * track scope, and produce `TranslationCall[]`. Both public entry
 * points delegate here so the scope and match logic stays in one
 * place.
 *
 * @param {ts.SourceFile} sourceFile
 * @param {string} filePath
 * @param {ts.TypeChecker | null} checker
 * @returns {TranslationCall[]}
 */
function walkSourceFile(sourceFile, filePath, checker) {
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
          const ref = extractKeyRef(node.arguments[0], checker);
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
 * Create a TypeScript `Program` rooted at the project's tsconfig.
 *
 * Building the Program is the expensive step (parses every file in
 * the compilation). The CLI creates it once and reuses the same
 * Program across every file scanned — amortising the cost over all
 * the cross-file type lookups.
 *
 * @param {string} tsconfigPath absolute path to tsconfig.json
 * @returns {ts.Program}
 */
export function createAuditProgram(tsconfigPath) {
  const cfg = ts.readConfigFile(tsconfigPath, (p) =>
    fs.readFileSync(p, "utf8"),
  );
  if (cfg.error) {
    const msg = ts.flattenDiagnosticMessageText(cfg.error.messageText, "\n");
    throw new Error(`Failed to read ${tsconfigPath}: ${msg}`);
  }
  const parsed = ts.parseJsonConfigFileContent(
    cfg.config,
    ts.sys,
    path.dirname(tsconfigPath),
  );
  return ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
  });
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
 * literals become exact keys; template literals with a resolvable
 * string-literal-union expression become a full `enum_prefix`;
 * anything else with a static prefix falls back to the parent-path
 * `prefix` form.
 *
 * @param {ts.Expression} arg
 * @param {ts.TypeChecker | null} checker
 * @returns {KeyRef | null}
 */
function extractKeyRef(arg, checker) {
  if (ts.isStringLiteralLike(arg)) {
    return { kind: "static", key: arg.text };
  }
  if (!ts.isTemplateExpression(arg)) return null;

  // `` `head${...}` `` — the raw `head` is the only stable literal.
  // We always support the bare "parent path" fallback:
  //
  //   `kind.${x}`           → head = "kind."     → parent = "kind"
  //   `readOnly.reason${x}` → head = "readOnly." → parent = "readOnly"
  //   `prefix${x}`          → head = "prefix"    → no dot → skip
  //
  // When a type checker is available AND the template is simple
  // (head + single expression + empty tail), resolve the expression's
  // type and, if it's a finite union of string literals, promote to
  // `enum_prefix` so `auditCalls` can verify every concrete leaf.
  const head = arg.head.text;
  const enumValues =
    checker && isSimpleOneHoleTemplate(arg)
      ? resolveStringLiteralUnion(checker, arg.templateSpans[0].expression)
      : null;

  if (enumValues && enumValues.length > 0) {
    // `enum_prefix` doesn't care whether the head ends with `.` —
    // we'll concatenate the raw head with each enum value below,
    // mirroring how the UI composes the key at runtime.
    return { kind: "enum_prefix", prefix: head, values: enumValues };
  }

  const lastDot = head.lastIndexOf(".");
  if (lastDot === -1) return null;
  const prefix = head.slice(0, lastDot);
  if (prefix.length === 0) return null;
  return { kind: "prefix", prefix };
}

/**
 * @param {ts.TemplateExpression} tpl
 * @returns {boolean}
 *
 * Narrow check for the common shape `` `head${x}` `` — head + a
 * single interpolation + empty tail. Multi-expression templates
 * like `` `a${x}.${y}` `` are skipped (concrete leaf values depend
 * on two independent unions, cartesian-product checking is more
 * work than it's worth at this stage).
 */
function isSimpleOneHoleTemplate(tpl) {
  if (tpl.templateSpans.length !== 1) return false;
  const span = tpl.templateSpans[0];
  return (
    ts.isTemplateTail(span.literal) && span.literal.text.length === 0
  );
}

/**
 * If `expr`'s static type is a union of string literals (inferred
 * from `as const` arrays, explicit union aliases, type predicates,
 * etc.), return the full list of literal values. Returns `null` for
 * any broader type — `string`, `string | number`, a non-exhaustive
 * union, or a type we can't flatten safely.
 *
 * @param {ts.TypeChecker} checker
 * @param {ts.Expression} expr
 * @returns {string[] | null}
 */
function resolveStringLiteralUnion(checker, expr) {
  const type = checker.getTypeAtLocation(expr);
  /** @type {string[]} */
  const out = [];

  const variants = type.isUnion() ? type.types : [type];
  for (const t of variants) {
    if (t.isStringLiteral()) {
      out.push(t.value);
      continue;
    }
    // Any non-literal member (plain `string`, `number`, etc.)
    // means we can't enumerate safely — bail out completely.
    return null;
  }
  return out.length > 0 ? out : null;
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

  /**
   * Check a single dotted path against en + ko and, if missing in
   * either, push a `Finding`. Returns `true` when both bundles
   * carry the path (caller uses this for `prefix_is_leaf` checks).
   *
   * @param {TranslationCall} call
   * @param {string} fullPath
   * @returns {{ enHit: unknown, koHit: unknown, present: boolean }}
   */
  function checkPath(call, fullPath) {
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
      return { enHit, koHit, present: false };
    }
    return { enHit, koHit, present: true };
  }

  for (const call of calls) {
    if (call.ref.kind === "enum_prefix") {
      // `enum_prefix` has one finding per missing concrete leaf —
      // e.g. if `readOnly.reason` has five variants and only four
      // keys exist, the scanner reports exactly the missing one.
      // We intentionally do NOT fall through to a prefix check here:
      // the parent may legitimately be flat (no `reason.*` sub-object).
      for (const value of call.ref.values) {
        const fullPath = `${call.namespace}.${call.ref.prefix}${value}`;
        checkPath(call, fullPath);
      }
      continue;
    }

    // `static` and bare `prefix` — build the full dotted path and
    // check once against both bundles. The ref is already normalised
    // (no trailing dot, no leading dot) so the join is a plain
    // concatenation.
    const tail =
      call.ref.kind === "static" ? call.ref.key : call.ref.prefix;
    const fullPath = tail ? `${call.namespace}.${tail}` : call.namespace;
    const { enHit, koHit, present } = checkPath(call, fullPath);
    if (!present) continue;

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

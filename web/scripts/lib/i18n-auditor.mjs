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
 * Scan a file for hard-coded user-facing strings sitting on
 * accessibility-critical JSX attributes (`placeholder`, `aria-label`,
 * `title`, `alt`, `aria-description`). These attributes are the most
 * common i18n leak vectors — they end up announced to screen readers
 * or shown in tooltips even when the rest of the surface is localised.
 *
 * Heuristics tuned to minimise false positives:
 *   - Only fires on string-literal attribute values; `{expression}`
 *     forms (e.g. `{t("foo")}`) are ignored entirely.
 *   - Empty strings and strings without any alpha character are
 *     skipped — those are presentation hints (`"•"`, `"→"`, `"---"`).
 *   - Single-token alphabetic acronyms ≤ 4 chars are allowed (covers
 *     "API", "PDF", "URL", "CSV", brand names like "AI") since
 *     translating them is a no-op.
 *   - `// i18n-audit-ignore` comment on the line immediately above an
 *     attribute opts that single occurrence out of the gate — useful
 *     for genuinely language-neutral strings the heuristic doesn't
 *     recognise (eg connection-string examples).
 *
 * @typedef {{ file: string, line: number, attribute: string, value: string }} HardcodedString
 *
 * @param {string} filePath
 * @param {string} source
 * @returns {HardcodedString[]}
 */
export function findHardcodedJsxStrings(filePath, source) {
  const sourceFile = ts.createSourceFile(
    filePath,
    source,
    ts.ScriptTarget.Latest,
    /* setParentNodes */ true,
    ts.ScriptKind.TSX,
  );
  /** @type {HardcodedString[]} */
  const out = [];

  // Collect line numbers of `// i18n-audit-ignore` comments so we can
  // suppress findings on the next non-comment line. Build the lookup
  // once per file rather than per attribute.
  /** @type {Set<number>} */
  const ignoredLines = new Set();
  // Walk every comment via a regex over the raw source — JSX scanner
  // needs to know the content's parsing context (which alternates
  // between JSX and TS), and getting that exactly right per token is
  // brittle. A line-by-line scan for the ignore marker is robust and
  // catches every comment shape (`//` between attributes, `{/* */}`
  // between elements, `/* */` block).
  //
  // Marker variants:
  //   `i18n-audit-ignore`            — suppress next 2 lines (default,
  //                                    covers attribute / single-line
  //                                    JSX text patterns)
  //   `i18n-audit-ignore(N)`         — suppress next N lines (used when
  //                                    a marker precedes a multi-line
  //                                    JSX element such as a `<select>`
  //                                    with many `<option>` children)
  const lines = source.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const match = /i18n-audit-ignore(?:\((\d+)\))?/.exec(lines[i]);
    if (!match) continue;
    const span = match[1] ? Math.min(parseInt(match[1], 10), 200) : 2;
    for (let k = 0; k <= span; k++) ignoredLines.add(i + k);
  }

  const FLAGGED_ATTRS = new Set([
    "placeholder",
    "aria-label",
    "aria-description",
    "title",
    "alt",
  ]);

  // Element names whose JSX text children are intentionally non-prose
  // (technical identifiers, code samples, keyboard glyphs). We look at
  // the *parent* JSX element's tag when deciding whether to flag a
  // text child — `<code>SELECT * FROM x</code>` and `<kbd>Enter</kbd>`
  // are NOT i18n leaks even though they contain alpha sequences.
  const TECHNICAL_ELEMENTS = new Set([
    "code",
    "pre",
    "kbd",
    "tt",
    "samp",
    "var",
  ]);

  /** @param {string} value */
  const looksLikeUserProse = (value) => {
    const trimmed = value.trim();
    if (trimmed.length === 0) return false;
    // Strip HTML entities (`&rarr;`, `&nbsp;`, `&#8594;`) before the
    // alpha check — these render as Unicode glyphs at runtime, so the
    // *text content* is non-prose even though the source contains
    // `rarr` etc.
    const decoded = trimmed.replace(/&[a-z][a-z0-9]+;|&#\d+;/gi, "");
    if (!/[a-zA-Z]/.test(decoded)) return false;
    // ≤ 4-char all-uppercase acronyms ("API", "URL", "PDF", "CSV")
    // are language-neutral technical labels; skip.
    if (/^[A-Z0-9_]{2,4}$/.test(trimmed)) return false;
    // Multi-word prose ("Save changes" / "Click to edit") is always
    // user-facing.
    if (trimmed.includes(" ")) return true;
    // Single-word prose: 4+ contiguous alpha chars catches common UI
    // verbs ("Save", "Edit", "Done", "Cancel", "Submit"). Identifiers
    // mixed with non-alpha (`tenant_id`, `cs-order-status`) fall
    // through because the alpha run is shorter than 4.
    if (/[a-zA-Z]{4,}/.test(trimmed)) return true;
    return false;
  };

  /**
   * Resolve the tag name of the JSX element / fragment that owns this
   * node. Used to skip text inside `<code>`, `<pre>`, `<kbd>` etc.
   * Returns `null` when the parent isn't a JSX element (e.g. fragment).
   * @param {ts.Node} node
   * @returns {string | null}
   */
  const enclosingJsxTag = (node) => {
    let cur = node.parent;
    while (cur) {
      if (ts.isJsxElement(cur)) {
        const tag = cur.openingElement.tagName;
        if (ts.isIdentifier(tag)) return tag.text;
        return null;
      }
      if (ts.isJsxFragment(cur)) return null;
      cur = cur.parent;
    }
    return null;
  };

  /** Common report path so attribute + text findings share the
   *  ignore-line and ts ranges logic.
   *  @param {ts.Node} reportNode
   *  @param {string} attribute
   *  @param {string} value */
  const reportLeak = (reportNode, attribute, value) => {
    const { line } = sourceFile.getLineAndCharacterOfPosition(
      reportNode.getStart(sourceFile),
    );
    if (ignoredLines.has(line)) return;
    out.push({
      file: filePath,
      line: line + 1,
      attribute,
      value,
    });
  };

  /** @param {ts.Node} node */
  const visit = (node) => {
    // 1. Attribute literals (placeholder, aria-label, ...).
    if (ts.isJsxAttribute(node) && ts.isIdentifier(node.name)) {
      const attrName = node.name.text;
      if (FLAGGED_ATTRS.has(attrName) && node.initializer) {
        // `attr="literal"` — directly a StringLiteral.
        // `attr={"literal"}` — JsxExpression wrapping a StringLiteral.
        let literal = null;
        if (ts.isStringLiteral(node.initializer)) {
          literal = node.initializer;
        } else if (
          ts.isJsxExpression(node.initializer) &&
          node.initializer.expression &&
          ts.isStringLiteral(node.initializer.expression)
        ) {
          literal = node.initializer.expression;
        }
        if (literal && looksLikeUserProse(literal.text)) {
          reportLeak(literal, attrName, literal.text);
        }
      }
    }

    // 2. Bare JSX text content (`<p>Hello world</p>`). Skip
    // `<code>` / `<pre>` / `<kbd>` etc. — those carry technical
    // strings by design. Expression children (`{t("foo")}`) are
    // a different node kind and never reach this branch.
    if (ts.isJsxText(node)) {
      const text = node.text;
      if (looksLikeUserProse(text)) {
        const tag = enclosingJsxTag(node);
        if (!(tag && TECHNICAL_ELEMENTS.has(tag))) {
          reportLeak(node, "<text>", text.trim());
        }
      }
    }

    // 3. JSX-expression-wrapped string literal text
    // (`<p>{"Hello world"}</p>`). Same content, different node shape.
    if (
      ts.isJsxExpression(node) &&
      node.expression &&
      ts.isStringLiteral(node.expression) &&
      node.parent &&
      (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))
    ) {
      const literal = node.expression;
      if (looksLikeUserProse(literal.text)) {
        const tag = enclosingJsxTag(node);
        if (!(tag && TECHNICAL_ELEMENTS.has(tag))) {
          reportLeak(literal, "<text>", literal.text);
        }
      }
    }

    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return out;
}

/**
 * Scan `src/app/.../page.tsx` files and return any that render JSX
 * without calling `useTranslations` — these pages are guaranteed to
 * carry hard-coded copy in violation of the i18n policy. Heuristic:
 * pages that render *only* a child component (e.g. `<RecipesWorkbench />`)
 * are exempt because their copy lives in the child.
 *
 * @typedef {{ file: string, line: number }} UntranslatedPage
 *
 * @param {string} rootDir absolute path to `web/src`
 * @returns {UntranslatedPage[]}
 */
export function findPagesMissingI18n(rootDir) {
  /** @type {UntranslatedPage[]} */
  const out = [];
  const appDir = path.join(rootDir, "app");
  if (!fs.existsSync(appDir)) return out;

  /** @param {string} dir */
  const recurse = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === "node_modules" || entry.name === ".next") continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        recurse(full);
        continue;
      }
      if (entry.name !== "page.tsx" || full.includes("__tests__")) continue;
      const source = fs.readFileSync(full, "utf8");
      // Quick reject: file calls useTranslations anywhere.
      if (/\buseTranslations\b/.test(source)) continue;
      // Render-only-child shells (`return <X />`) carry no copy — exempt.
      if (isRenderOnlyChildShell(source)) continue;
      out.push({ file: full, line: 1 });
    }
  };
  recurse(appDir);
  return out;
}

/**
 * True when a page carries no copy of its own — either it just
 * `redirect()`s, or it forwards to a child component that owns the
 * i18n surface. Both patterns are exempt from the
 * `useTranslations` requirement.
 *
 * Patterns recognised:
 *   - Body has no JSX at all (typical for `redirect()`-only pages)
 *   - Body's terminal statement is `return <ChildComponent ... />`
 *   - Body's terminal statement is `return <Tag>...</Tag>` whose
 *     children are all JSX element nodes (no text). Text children
 *     would be hard-coded copy and disqualify the shell.
 *
 * @param {string} source
 * @returns {boolean}
 */
function isRenderOnlyChildShell(source) {
  // Cheap fast-path: if the source has no JSX at all (no `</` close
  // tags and no `<Capital` opener), it's a redirect-style shell.
  if (!/<\/|<[A-Z]/.test(source)) return true;

  const sf = ts.createSourceFile(
    "page.tsx",
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  );
  let isShell = false;
  /** @param {ts.Node} node */
  const visit = (node) => {
    if (
      ts.isFunctionDeclaration(node) &&
      node.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword) &&
      node.modifiers?.some((m) => m.kind === ts.SyntaxKind.DefaultKeyword) &&
      node.body
    ) {
      const stmts = node.body.statements;
      const last = stmts[stmts.length - 1];
      if (!last || !ts.isReturnStatement(last)) return;
      const expr = last.expression;
      if (!expr) return;
      if (ts.isJsxSelfClosingElement(expr)) {
        isShell = true;
        return;
      }
      if (ts.isJsxElement(expr)) {
        const allElementChildren = expr.children.every(
          (c) =>
            ts.isJsxElement(c) ||
            ts.isJsxSelfClosingElement(c) ||
            ts.isJsxFragment(c) ||
            (ts.isJsxText(c) && !c.text.trim()),
        );
        if (allElementChildren) isShell = true;
      }
    }
  };
  ts.forEachChild(sf, visit);
  return isShell;
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

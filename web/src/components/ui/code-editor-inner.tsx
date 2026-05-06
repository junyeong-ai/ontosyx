"use client";

// `<CodeEditor>` — first-class code-editor primitive.
//
// CodeMirror 6 wrapped behind a small surface so consumers don't
// thread the EditorView lifecycle, the theme glue, or the
// language-extension wiring on every adoption. Three concerns
// the primitive owns:
//
//   1. **Theme** — the editor reads the design-system semantic
//      tokens via CSS variables so a workspace re-theme propagates
//      without forking the editor styles. Light + dark mode share
//      one definition; the active palette flips automatically with
//      the document `class="dark"` attribute.
//
//   2. **Language** — `language` selects the highlighter/parser:
//      `markdown` for prompt editing, `plain` for log preview /
//      ad-hoc text, `cypher` for graph queries (hand-rolled
//      lightweight tokenizer — no upstream Cypher grammar package),
//      `sql` for source-system query editing, `json` for IR /
//      payload editing.
//
//   3. **Lifecycle** — readonly / placeholder / external value sync
//      are all wired here so consumer code is just `<CodeEditor
//      value={…} onChange={…} language="cypher" />`.

import { useEffect, useMemo, useRef } from "react";
import {
  EditorView,
  keymap,
  lineNumbers,
  placeholder as placeholderExt,
} from "@codemirror/view";
import { EditorState, type Extension } from "@codemirror/state";
import {
  autocompletion,
  completionKeymap,
  type CompletionSource,
} from "@codemirror/autocomplete";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { sql } from "@codemirror/lang-sql";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { oneDark } from "@codemirror/theme-one-dark";
import {
  HighlightStyle,
  StreamLanguage,
  syntaxHighlighting,
  defaultHighlightStyle,
} from "@codemirror/language";
import { tags } from "@lezer/highlight";

// ---------------------------------------------------------------------------
// Theme — sourced from design-system CSS variables. The CSS-var
// references render with whatever palette is active at the
// container's mount, so light/dark mode + future workspace themes
// reflect without re-rendering the editor.
// ---------------------------------------------------------------------------

const editorTheme = EditorView.theme({
  "&": {
    fontSize: "13px",
    backgroundColor: "var(--color-surface-base)",
    color: "var(--color-foreground)",
  },
  ".cm-content": {
    fontFamily: "var(--font-mono)",
    caretColor: "var(--color-brand-foreground)",
  },
  ".cm-gutters": {
    backgroundColor: "var(--color-surface-raised)",
    color: "var(--color-foreground-subtle)",
    border: "none",
    borderRight: "1px solid var(--color-divider)",
  },
  "&.cm-focused .cm-cursor": {
    borderLeftColor: "var(--color-brand-foreground)",
  },
  "&.cm-focused": {
    outline: "2px solid var(--color-brand-foreground)",
    outlineOffset: "-2px",
  },
  ".cm-selectionBackground, ::selection": {
    backgroundColor: "var(--color-brand-surface-strong) !important",
  },
  ".cm-activeLine": {
    backgroundColor: "var(--color-surface-raised-muted)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "var(--color-surface-raised-muted)",
  },
});

// ---------------------------------------------------------------------------
// Cypher tokenizer (lightweight)
// ---------------------------------------------------------------------------
//
// CodeMirror has no upstream Cypher language package. The grammar
// is small enough that a `StreamLanguage` tokenizer is plenty for
// syntax-highlighting purposes — we want keywords / strings /
// numbers / labels visually distinct, not a full parse tree.

const CYPHER_KEYWORDS = new Set([
  "match",
  "where",
  "return",
  "create",
  "merge",
  "delete",
  "detach",
  "set",
  "remove",
  "with",
  "unwind",
  "order",
  "by",
  "asc",
  "desc",
  "limit",
  "skip",
  "as",
  "and",
  "or",
  "not",
  "in",
  "is",
  "null",
  "true",
  "false",
  "case",
  "when",
  "then",
  "else",
  "end",
  "call",
  "yield",
  "optional",
  "distinct",
  "count",
  "collect",
  "exists",
  "any",
  "all",
  "none",
  "single",
  "size",
]);

const cypherStream = StreamLanguage.define({
  name: "cypher",
  startState: () => ({}),
  token(stream) {
    if (stream.eatSpace()) return null;
    // String literal — single or double quote.
    if (stream.match(/^"(?:[^"\\]|\\.)*"/) || stream.match(/^'(?:[^'\\]|\\.)*'/)) {
      return "string";
    }
    // Backtick-quoted identifier (Cypher escapes property names this way).
    if (stream.match(/^`[^`]*`/)) return "name";
    // Number — int or float.
    if (stream.match(/^-?\d+(?:\.\d+)?/)) return "number";
    // Comment to end of line.
    if (stream.match(/^\/\/.*/)) return "comment";
    // Variable / keyword / identifier.
    const word = stream.match(/^[A-Za-z_][A-Za-z0-9_]*/) as
      | RegExpMatchArray
      | null
      | true;
    if (word && Array.isArray(word)) {
      const text = word[0].toLowerCase();
      return CYPHER_KEYWORDS.has(text) ? "keyword" : "variable";
    }
    // Operator / punctuation — single char, no token style.
    stream.next();
    return null;
  },
  tokenTable: {
    keyword: tags.keyword,
    string: tags.string,
    number: tags.number,
    comment: tags.comment,
    variable: tags.variableName,
    name: tags.propertyName,
  },
});

const cypherHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--color-brand-foreground)", fontWeight: "600" },
  { tag: tags.string, color: "var(--color-warning-foreground)" },
  { tag: tags.number, color: "var(--color-info-foreground)" },
  { tag: tags.comment, color: "var(--color-foreground-subtle)", fontStyle: "italic" },
  { tag: tags.variableName, color: "var(--color-foreground-strong)" },
  { tag: tags.propertyName, color: "var(--color-concept-foreground)" },
]);

// ---------------------------------------------------------------------------
// Language registry
// ---------------------------------------------------------------------------

export type CodeLanguage = "markdown" | "cypher" | "sql" | "json" | "plain";

function languageExtensions(lang: CodeLanguage, isDark: boolean): Extension[] {
  const baseHighlight = isDark
    ? []
    : [syntaxHighlighting(defaultHighlightStyle)];
  switch (lang) {
    case "markdown":
      return [markdown(), ...baseHighlight];
    case "cypher":
      return [cypherStream, syntaxHighlighting(cypherHighlight)];
    case "sql":
      return [sql(), ...baseHighlight];
    case "json":
      return [json(), ...baseHighlight];
    case "plain":
      return [];
  }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface CodeEditorProps {
  value: string;
  onChange?: (value: string) => void;
  /**
   * Editor height. CSS string — a fixed height (`"400px"`) makes
   * sense for a settings page editor; a viewport-relative height
   * (`"40vh"`) is the right call inside a panel that should
   * proportion with surrounding chrome.
   */
  height?: string;
  /** When true, the editor renders content but blocks edits. */
  readOnly?: boolean;
  /** Empty-state copy shown when value is empty. */
  placeholder?: string;
  /** Default `markdown` for legacy callers; `cypher` / `plain` available. */
  language?: CodeLanguage;
  /** Hide the gutter (line numbers + active-line highlight). */
  hideLineNumbers?: boolean;
  /**
   * Optional aria-label exposed on the editor wrapper for screen
   * readers when the surrounding chrome doesn't already own a label.
   */
  ariaLabel?: string;
  /**
   * Optional autocomplete source. When set, the editor wires the
   * `@codemirror/autocomplete` extension and uses this source as
   * the suggestion provider. Caller composes it from the workspace
   * ontology via `makeCypherCompletionSource(catalog)` so the
   * editor primitive stays decoupled from the ontology shape.
   */
  completionSource?: CompletionSource;
}

export function CodeEditor({
  value,
  onChange,
  height = "400px",
  readOnly = false,
  placeholder,
  language = "markdown",
  hideLineNumbers = false,
  ariaLabel,
  completionSource,
}: CodeEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  // Sync the latest `onChange` into the ref after every render so
  // CodeMirror's `updateListener` (mounted once) reads the current
  // callback without the editor re-mounting on every callback
  // identity change. The ref pattern is the React 19 sanctioned
  // way to bridge "stable mount lifecycle" + "fresh callback".
  useEffect(() => {
    onChangeRef.current = onChange;
  });

  // The mount-time extensions array is the one place we read the
  // current dark-mode state. CSS-variable theme tokens take care of
  // every other palette change without remount; the dark-only
  // `oneDark` extension is the exception because it ships its own
  // syntax-highlight palette.
  const isDarkOnMount = useMemo(() => {
    if (typeof document === "undefined") return false;
    return document.documentElement.classList.contains("dark");
  }, []);

  useEffect(() => {
    if (!containerRef.current) return;

    const extensions: Extension[] = [
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap, ...completionKeymap]),
      ...languageExtensions(language, isDarkOnMount),
      isDarkOnMount ? oneDark : editorTheme,
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current?.(update.state.doc.toString());
        }
      }),
    ];

    if (completionSource) {
      // Wire the autocomplete extension only when a source is
      // supplied — bare callers (markdown / json prompt editor)
      // don't pay the dropdown surface area.
      extensions.push(
        autocompletion({
          override: [completionSource],
          // `closeOnBlur: true` keeps the menu out of the user's
          // way when they click outside the editor; the Cypher
          // surface uses keyboard-only navigation when the menu's
          // open.
          closeOnBlur: true,
        }),
      );
    }
    if (!hideLineNumbers) extensions.push(lineNumbers());
    if (readOnly) extensions.push(EditorState.readOnly.of(true));
    if (placeholder) extensions.push(placeholderExt(placeholder));

    const state = EditorState.create({ doc: value, extensions });
    const view = new EditorView({
      state,
      parent: containerRef.current,
    });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Re-mount when readOnly / language / line-number visibility changes —
    // these affect the extension list. `value` syncs through the next
    // effect so a typing burst doesn't re-mount.
  }, [
    readOnly,
    language,
    hideLineNumbers,
    value,
    placeholder,
    isDarkOnMount,
    completionSource,
  ]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  // The wrapper hosts CodeMirror's actual editor surface, which
  // already exposes a contenteditable region with proper focusable
  // semantics + keyboard handling. We forward the optional aria-label
  // here for screen readers that announce the surrounding region;
  // the role lives on the CodeMirror-provided editable child where
  // it's actually focusable.
  return (
    <div
      ref={containerRef}
      aria-label={ariaLabel}
      className="overflow-hidden rounded-md border border-divider"
      style={{ height }}
    />
  );
}

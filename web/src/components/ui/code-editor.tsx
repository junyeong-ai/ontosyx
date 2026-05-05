"use client";

// `<CodeEditor>` — lazy-loaded façade around the CodeMirror-backed
// editor implementation.
//
// CodeMirror 6 + the language packages it bundles (`markdown`,
// `json`, `theme-one-dark`, our hand-rolled cypher tokenizer) total
// ~80kb gzipped. Only ~3 surfaces in the workbench actually render
// the editor (settings/prompts, query-panel, future SHACL editor),
// and the user typically navigates to them after the initial app
// shell paints. Eager import puts the entire CodeMirror bundle on
// the critical path for every page load — including pages that
// never render an editor at all.
//
// `next/dynamic` splits CodeMirror into its own chunk that ships on
// demand. Pre-load via the editor's component file is implicit:
// every consumer's `import { CodeEditor } from "@/components/ui/code-editor"`
// stays as-is, but Webpack/Turbopack now treats the implementation
// as a separate chunk that streams in when the dynamic import
// resolves.
//
// `ssr: false` is intentional — CodeMirror creates DOM ref handles
// and registers `keydown` listeners during construction, neither of
// which are valid during server render. Without `ssr: false` the
// build works but every editor surface flashes a hydration mismatch
// before settling.
//
// `loading` renders a fixed-height skeleton matching the editor's
// chrome so the surrounding layout doesn't shift when CodeMirror
// resolves. The skeleton honours the `height` prop the consumer
// would have passed to the editor; we default to the editor's own
// fallback so a forgotten height still renders sensibly.

import dynamic from "next/dynamic";
import type { ComponentProps } from "react";

import type { CodeEditor as CodeEditorImpl } from "./code-editor-inner";

type CodeEditorProps = ComponentProps<typeof CodeEditorImpl>;

const LazyCodeEditor = dynamic(
  () => import("./code-editor-inner").then((m) => m.CodeEditor),
  {
    ssr: false,
    loading: () => (
      <div
        role="status"
        className="animate-pulse rounded-md border border-divider bg-surface-raised"
        style={{ height: "400px" }}
      />
    ),
  },
);

export function CodeEditor(props: CodeEditorProps) {
  return <LazyCodeEditor {...props} />;
}

export type { CodeLanguage } from "./code-editor-inner";

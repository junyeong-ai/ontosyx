"use client";

import { useEffect, useRef } from "react";
import { EditorView, keymap, lineNumbers, placeholder as placeholderExt } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { markdown } from "@codemirror/lang-markdown";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { oneDark } from "@codemirror/theme-one-dark";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";

// Zinc-based light theme matching the app's design tokens
const lightTheme = EditorView.theme({
  "&": {
    fontSize: "12px",
    backgroundColor: "rgb(250 250 250)", // zinc-50
  },
  ".cm-content": {
    fontFamily: "var(--font-geist-mono), monospace",
    caretColor: "rgb(16 185 129)", // emerald-500
  },
  ".cm-gutters": {
    backgroundColor: "rgb(244 244 245)", // zinc-100
    color: "rgb(161 161 170)", // zinc-400
    borderRight: "1px solid rgb(228 228 231)", // zinc-200
  },
  "&.cm-focused .cm-cursor": {
    borderLeftColor: "rgb(16 185 129)",
  },
  "&.cm-focused": {
    outline: "2px solid rgb(16 185 129 / 0.5)",
    outlineOffset: "-1px",
  },
  ".cm-selectionBackground": {
    backgroundColor: "rgb(16 185 129 / 0.15) !important",
  },
  ".cm-activeLine": {
    backgroundColor: "rgb(244 244 245 / 0.5)", // zinc-100/50
  },
  ".cm-activeLineGutter": {
    backgroundColor: "rgb(244 244 245)", // zinc-100
  },
});

interface CodeEditorProps {
  value: string;
  onChange?: (value: string) => void;
  height?: string;
  readOnly?: boolean;
  placeholder?: string;
}

export function CodeEditor({
  value,
  onChange,
  height = "400px",
  readOnly = false,
  placeholder,
}: CodeEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!containerRef.current) return;

    // Detect dark mode
    const isDark = document.documentElement.classList.contains("dark");

    const extensions = [
      lineNumbers(),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      markdown(),
      isDark
        ? oneDark
        : [lightTheme, syntaxHighlighting(defaultHighlightStyle)],
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current?.(update.state.doc.toString());
        }
      }),
    ];

    if (readOnly) {
      extensions.push(EditorState.readOnly.of(true));
    }

    if (placeholder) {
      extensions.push(placeholderExt(placeholder));
    }

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Only run on mount/unmount — value sync handled below
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [readOnly]);

  // Sync external value changes (e.g., switching between templates)
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

  return (
    <div
      ref={containerRef}
      className="overflow-hidden rounded-md border border-zinc-200 dark:border-zinc-700"
      style={{ height }}
    />
  );
}

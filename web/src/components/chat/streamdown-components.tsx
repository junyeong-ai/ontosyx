"use client";

import {
  type ComponentType,
  type HTMLAttributes,
  type TdHTMLAttributes,
  type ThHTMLAttributes,
  useState,
  useCallback,
  useEffect,
} from "react";
import { createPortal } from "react-dom";
import { useTranslations } from "next-intl";

// ---------------------------------------------------------------------------
// Custom Streamdown Components — dark-mode ontology UI, portal fullscreen
// ---------------------------------------------------------------------------

// ── Table wrapper with portal-based fullscreen ─────────────────
// streamdown's built-in fullscreen uses position:fixed inside the message
// bubble, which gets clipped by overflow:hidden ancestors.
// This wrapper uses React Portal to render at document.body level.

export const TableWrapper: ComponentType<HTMLAttributes<HTMLTableElement>> = ({
  children,
  ...props
}) => {
  const t = useTranslations("workbench.chat.streamdown");
  const [isFullscreen, setIsFullscreen] = useState(false);

  const close = useCallback(() => setIsFullscreen(false), []);

  // ESC key closes fullscreen
  useEffect(() => {
    if (!isFullscreen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [isFullscreen, close]);

  const table = (
    <table
      className="w-full border-collapse text-sm"
      {...props}
    >
      {children}
    </table>
  );

  return (
    <>
      {/* Inline view */}
      <div className="group/table relative my-2 overflow-x-auto rounded-lg border border-divider">
        {/* Fullscreen button */}
        <button
          type="button"
          onClick={() => setIsFullscreen(true)}
          className="absolute end-2 top-2 z-canvas rounded p-1 text-foreground-muted opacity-0 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset hover:text-foreground group-hover/table:opacity-100-muted"
          aria-label={t("viewFullscreen")}
          title={t("expandTable")}
        >
          <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5} aria-hidden="true">
            <path strokeLinecap="round" strokeLinejoin="round" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5v-4m0 4h-4m4 0l-5-5" />
          </svg>
        </button>
        {table}
      </div>

      {/* Portal fullscreen modal */}
      {isFullscreen &&
        createPortal(
          <div
            className="fixed inset-0 z-[9999] flex flex-col bg-surface-base"
            role="dialog"
            aria-modal="true"
            aria-label={t("tableFullscreenAria")}
          >
            {/* Header */}
            <div className="flex items-center justify-end border-b border-divider px-4 py-2">
              <button
                type="button"
                onClick={close}
                className="rounded-md p-1.5 text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset hover:text-foreground-strong"
                aria-label={t("closeFullscreen")}
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5} aria-hidden="true">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            {/* Scrollable table */}
            <div className="flex-1 overflow-auto p-4">
              <table className="w-full border-collapse text-sm" {...props}>
                {children}
              </table>
            </div>
          </div>,
          document.body
        )}
    </>
  );
};

// ── Table sections ─────────────────────────────────────────────

export const TableHead: ComponentType<HTMLAttributes<HTMLTableSectionElement>> = ({
  children,
  ...props
}) => (
  <thead
    className="sticky top-0 z-canvas border-b border-divider bg-surface-raised"
    {...props}
  >
    {children}
  </thead>
);

export const TableHeaderCell: ComponentType<ThHTMLAttributes<HTMLTableCellElement>> = ({
  children,
  ...props
}) => (
  <th
    className="whitespace-nowrap px-3 py-2 text-start text-xs font-semibold uppercase tracking-wider text-foreground-muted"
    {...props}
  >
    {children}
  </th>
);

export const TableCell: ComponentType<TdHTMLAttributes<HTMLTableCellElement>> = ({
  children,
  ...props
}) => (
  <td
    className="border-b border-divider-soft px-3 py-2 text-foreground-muted"
    {...props}
  >
    {children}
  </td>
);

export const TableRow: ComponentType<HTMLAttributes<HTMLTableRowElement>> = ({
  children,
  ...props
}) => (
  <tr
    className="transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-raised"
    {...props}
  >
    {children}
  </tr>
);

// ── Code blocks ────────────────────────────────────────────────
// Replaces streamdown's default wrapper (which adds extra divs with
// bright borders). Single <pre> block with subtle border, matching
// the memory-poc pattern.

export const CodeBlock: ComponentType<HTMLAttributes<HTMLPreElement>> = ({
  children,
  ...props
}) => (
  <pre
    className="my-2 overflow-x-auto whitespace-pre-wrap break-words rounded-lg border border-divider bg-surface-base p-3 text-[0.75rem] leading-relaxed text-foreground-strong"
    {...props}
  >
    {children}
  </pre>
);

// ── Code (inline vs block) ─────────────────────────────────────
// Block code (language-*) passes through to <pre> for Shiki rendering.
// Inline code gets a compact styled badge.

export const Code: ComponentType<HTMLAttributes<HTMLElement> & { className?: string }> = ({
  className,
  children,
  ...props
}) => {
  if (className?.startsWith("language-")) {
    return <code className={className} {...props}>{children}</code>;
  }
  return (
    <code
      className="rounded bg-surface-inset px-1.5 py-0.5 font-mono text-[0.85em] text-brand-foreground"
      {...props}
    >
      {children}
    </code>
  );
};

// ── Links ──────────────────────────────────────────────────────

export const Link: ComponentType<HTMLAttributes<HTMLAnchorElement> & { href?: string }> = ({
  children,
  href,
  ...props
}) => (
  <a
    href={href}
    className="text-brand-foreground underline underline-offset-2 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-brand-foreground-strong"
    target={href?.startsWith("http") ? "_blank" : undefined}
    rel={href?.startsWith("http") ? "noopener noreferrer" : undefined}
    {...props}
  >
    {children}
  </a>
);

// ── Blockquote ─────────────────────────────────────────────────

export const Blockquote: ComponentType<HTMLAttributes<HTMLQuoteElement>> = ({
  children,
  ...props
}) => (
  <blockquote
    className="my-2 border-s-3 border-divider ps-3 text-foreground-muted"
    {...props}
  >
    {children}
  </blockquote>
);

// ── Export all components as a single config object ────────────

export const streamdownComponents = {
  pre: CodeBlock,
  code: Code,
  table: TableWrapper,
  thead: TableHead,
  th: TableHeaderCell,
  td: TableCell,
  tr: TableRow,
  a: Link,
  blockquote: Blockquote,
};

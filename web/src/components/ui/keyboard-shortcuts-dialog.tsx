"use client";

import { useState, useMemo } from "react";
import { useTranslations } from "next-intl";
import { FocusTrap } from "@/components/ui/focus-trap";

import { KeyboardShortcut } from "./keyboard-shortcut";

import {
  specGlyph,
  useShortcut,
  useShortcuts,
  type ShortcutSpec,
} from "@/lib/shortcuts";

export function KeyboardShortcutsDialog() {
  const t = useTranslations("keyboardShortcuts");
  const tGroups = useTranslations();
  const [isOpen, setIsOpen] = useState(false);
  const shortcuts = useShortcuts();

  // Toggle: `?` (the canonical "what are my shortcuts" key, present in
  // GitHub, Slack, Notion) plus `mod+/` for keyboards where `?` is hard
  // to reach. Registered through the same registry it reveals so the
  // help dialog itself appears in its own listing — discoverability
  // beats meta-trickiness here.
  useShortcut({
    id: "help.toggle",
    keys: ["shift+?", "mod+/"],
    group: "keyboardShortcuts.sections.global",
    description: "keyboardShortcuts.shortcuts.toggleHelp",
    handler: (e) => {
      e.preventDefault();
      setIsOpen((v) => !v);
    },
  });

  // Escape closes the dialog. Scoped guard so the global Escape
  // shortcut (e.g. exit-fullscreen) doesn't fire while the help is up.
  useShortcut({
    id: "help.close",
    keys: ["Escape"],
    group: "keyboardShortcuts.sections.global",
    description: "keyboardShortcuts.shortcuts.closeHelp",
    priority: 100,
    enabled: () => isOpen,
    handler: () => setIsOpen(false),
  });

  const grouped = useMemo(() => {
    const map = new Map<string, ShortcutSpec[]>();
    for (const s of shortcuts) {
      const bucket = map.get(s.group) ?? [];
      bucket.push(s);
      map.set(s.group, bucket);
    }
    return Array.from(map.entries());
  }, [shortcuts]);

  if (!isOpen) return null;

  return (
    <FocusTrap
      focusTrapOptions={{
        initialFocus: false,
        allowOutsideClick: true,
        escapeDeactivates: false,
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="kb-shortcuts-title"
        className="fixed inset-0 z-modal flex items-center justify-center bg-surface-overlay backdrop-blur-sm"
      >
        <button
          type="button"
          aria-label={t("closeAria")}
          className="absolute inset-0 cursor-default"
          onClick={() => setIsOpen(false)}
        />
        <div className="relative w-96 rounded-xl border border-divider bg-surface-base p-4 shadow-4">
          <h2
            id="kb-shortcuts-title"
            className="mb-3 text-sm font-semibold text-foreground-strong"
          >
            {t("title")}
          </h2>
          <div className="space-y-3">
            {grouped.map(([group, specs]) => (
              <Section key={group} heading={tGroups(group)}>
                {specs.map((s) => (
                  <Row
                    key={s.id}
                    label={tGroups(s.description)}
                    glyph={specGlyph(s)}
                  />
                ))}
              </Section>
            ))}
          </div>
          <p className="mt-3 text-center text-2xs text-foreground-muted">
            {t("toggleHint")}
          </p>
        </div>
      </div>
    </FocusTrap>
  );
}

function Section({
  heading,
  children,
}: {
  heading: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <p className="mb-1 px-0.5 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {heading}
      </p>
      <div className="space-y-1.5">{children}</div>
    </div>
  );
}

function Row({ label, glyph }: { label: string; glyph: string | undefined }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-xs text-foreground-muted">{label}</span>
      {glyph && <KeyboardShortcut glyph={glyph} />}
    </div>
  );
}

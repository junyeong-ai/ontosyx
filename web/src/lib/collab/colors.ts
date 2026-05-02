// Stable per-user colour for collaboration UI (presence avatars,
// remote cursors, lock rings). Every surface that visualises a
// collaborator reads from this single palette so the same person
// reads the same hue across avatar, cursor, and lock indicator.
//
// Each colour clears WCAG AA contrast (≥ 4.5:1) against white text
// at small sizes — the original 500-shade palette failed at
// 1.95–3.6:1 for amber / teal / orange, which is the same regression
// the project hit with `text-emerald-600` (see
// `feedback_axe_emerald_600_aa_fail.md`).

const PRESENCE_PALETTE = [
  "#0369a1", // sky-700      — 6.4:1
  "#047857", // emerald-700  — 5.7:1
  "#b45309", // amber-700    — 4.9:1
  "#b91c1c", // red-700      — 6.6:1
  "#6d28d9", // violet-700   — 7.0:1
  "#be185d", // pink-700     — 6.5:1
  "#0f766e", // teal-700     — 5.8:1
  "#c2410c", // orange-700   — 5.5:1
] as const;

/**
 * Deterministic colour for a user id. The hash maps every id into
 * the palette so the same user keeps the same hue across sessions
 * and devices — important for cognitive recognition during
 * realtime collaboration.
 */
export function colorFor(userId: string): string {
  let h = 0;
  for (let i = 0; i < userId.length; i++) {
    h = (h * 31 + userId.charCodeAt(i)) >>> 0;
  }
  return PRESENCE_PALETTE[h % PRESENCE_PALETTE.length];
}

/** Number of distinct hues — useful for tests + density audits. */
export const PRESENCE_PALETTE_SIZE = PRESENCE_PALETTE.length;

// Stable per-user colour for collaboration UI (presence avatars,
// remote cursors, lock rings). Every surface that visualises a
// collaborator reads from this single helper so the same person
// reads the same hue across avatar, cursor, and lock indicator.
//
// The actual hex values live as `--collab-presence-{1..8}` CSS
// custom properties in `app/globals.css` — that's where light /
// dark variants belong, and where a future design-system pass
// will wire the palette into Tailwind theme tokens. This module
// just hashes a user id into a slot and hands back the matching
// `var(...)` reference.

/** Slots correspond to the eight `--collab-presence-{1..8}` tokens. */
const PRESENCE_TOKENS = [
  "var(--collab-presence-1)",
  "var(--collab-presence-2)",
  "var(--collab-presence-3)",
  "var(--collab-presence-4)",
  "var(--collab-presence-5)",
  "var(--collab-presence-6)",
  "var(--collab-presence-7)",
  "var(--collab-presence-8)",
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
  return PRESENCE_TOKENS[h % PRESENCE_TOKENS.length];
}

/** Number of distinct hues — useful for tests + density audits. */
export const PRESENCE_PALETTE_SIZE = PRESENCE_TOKENS.length;

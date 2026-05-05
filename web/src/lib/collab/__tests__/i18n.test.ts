// Coverage check — every collaboration message key the FE
// references must have a localised string in both `en` and `ko`.
// The stable lists below are kept in sync with the FE source: a
// new `ConnectionState`, `ErrorCode`, lock state, or action all
// land here too, and the test fails if either catalogue drops a
// key.

import { describe, it, expect } from "vitest";

import en from "../../../../messages/en.json";
import ko from "../../../../messages/ko.json";

/** Mirrors `crate::collaboration::ErrorCode`. */
const ERROR_CODES = [
  "auth_required",
  "auth_invalid",
  "auth_unavailable",
  "auth_timeout",
  "malformed_frame",
  "unauthorized_workspace",
  "unauthorized_ontology_draft",
  "too_many_connections",
  "broadcast_lagged",
  "not_joined",
  "session_revoked",
] as const;

/** Mirrors the `ConnectionState` union in `lib/collab/client.ts`. */
const STATUS_KEYS = [
  "idle",
  "connecting",
  "authenticating",
  "ready",
  "reconnecting",
  "closed",
] as const;

/** Lock-status messages rendered by `<LockIndicator>` and
 *  `<LockedByOtherBanner>`. */
const LOCK_KEYS = ["editingByYou", "editingBy"] as const;

/** Action labels (sonner action buttons, etc.). */
const ACTION_KEYS = ["signInAgain"] as const;

/**
 * Error messages carry a structured `{ title, description? }` —
 * the toaster renders title as the heading and description as the
 * detail line. All other groups (status, lock, actions) are flat
 * strings.
 */
interface ErrorMessage {
  title: string;
  description?: string;
}

interface CollaborationCatalogue {
  collaboration: {
    errors: Record<string, ErrorMessage>;
    status: Record<string, string>;
    lock: Record<string, string>;
    actions: Record<string, string>;
  };
}

function assertString(
  locale: string,
  group: string,
  key: string,
  value: unknown,
) {
  expect(value, `missing ${locale} key collaboration.${group}.${key}`)
    .toBeTypeOf("string");
  expect(
    (value as string)?.length,
    `empty ${locale} key collaboration.${group}.${key}`,
  ).toBeGreaterThan(0);
}

function assertFlatGroup(
  locale: string,
  bundle: CollaborationCatalogue,
  group: "status" | "lock" | "actions",
  keys: readonly string[],
) {
  const messages = bundle.collaboration[group];
  for (const key of keys) {
    assertString(locale, group, key, messages[key]);
  }
}

function assertErrorGroup(
  locale: string,
  bundle: CollaborationCatalogue,
  keys: readonly string[],
) {
  const messages = bundle.collaboration.errors;
  for (const key of keys) {
    const entry = messages[key];
    expect(entry, `missing ${locale} key collaboration.errors.${key}`).toBeTypeOf(
      "object",
    );
    assertString(locale, "errors", `${key}.title`, entry?.title);
    if (entry?.description !== undefined) {
      assertString(locale, "errors", `${key}.description`, entry.description);
    }
  }
}

describe("collaboration i18n coverage", () => {
  const enBundle = en as unknown as CollaborationCatalogue;
  const koBundle = ko as unknown as CollaborationCatalogue;

  it("every ErrorCode has en + ko messages with title + optional description", () => {
    assertErrorGroup("en", enBundle, ERROR_CODES);
    assertErrorGroup("ko", koBundle, ERROR_CODES);
  });

  it("every ConnectionState has en + ko status messages", () => {
    assertFlatGroup("en", enBundle, "status", STATUS_KEYS);
    assertFlatGroup("ko", koBundle, "status", STATUS_KEYS);
  });

  it("every lock-state message has en + ko translations", () => {
    assertFlatGroup("en", enBundle, "lock", LOCK_KEYS);
    assertFlatGroup("ko", koBundle, "lock", LOCK_KEYS);
  });

  it("every collaboration action label has en + ko translations", () => {
    assertFlatGroup("en", enBundle, "actions", ACTION_KEYS);
    assertFlatGroup("ko", koBundle, "actions", ACTION_KEYS);
  });
});

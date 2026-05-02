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
  "unauthorized_project",
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

interface CollaborationCatalogue {
  collaboration: {
    errors: Record<string, string>;
    status: Record<string, string>;
    lock: Record<string, string>;
    actions: Record<string, string>;
  };
}

function assertCovered(
  locale: string,
  bundle: CollaborationCatalogue,
  group: keyof CollaborationCatalogue["collaboration"],
  keys: readonly string[],
) {
  const messages = bundle.collaboration[group];
  for (const key of keys) {
    expect(
      messages[key],
      `missing ${locale} key collaboration.${group}.${key}`,
    ).toBeTypeOf("string");
    expect(
      messages[key]?.length,
      `empty ${locale} key collaboration.${group}.${key}`,
    ).toBeGreaterThan(0);
  }
}

describe("collaboration i18n coverage", () => {
  const enBundle = en as unknown as CollaborationCatalogue;
  const koBundle = ko as unknown as CollaborationCatalogue;

  it("every ErrorCode has en + ko messages", () => {
    assertCovered("en", enBundle, "errors", ERROR_CODES);
    assertCovered("ko", koBundle, "errors", ERROR_CODES);
  });

  it("every ConnectionState has en + ko status messages", () => {
    assertCovered("en", enBundle, "status", STATUS_KEYS);
    assertCovered("ko", koBundle, "status", STATUS_KEYS);
  });

  it("every lock-state message has en + ko translations", () => {
    assertCovered("en", enBundle, "lock", LOCK_KEYS);
    assertCovered("ko", koBundle, "lock", LOCK_KEYS);
  });

  it("every collaboration action label has en + ko translations", () => {
    assertCovered("en", enBundle, "actions", ACTION_KEYS);
    assertCovered("ko", koBundle, "actions", ACTION_KEYS);
  });
});

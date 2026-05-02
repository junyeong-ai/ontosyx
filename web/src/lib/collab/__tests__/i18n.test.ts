// Coverage check — every `ErrorCode` variant the server may emit
// must have a localised message in both `en` and `ko`. The test
// fails if the server adds a new variant without updating
// messages, or a translator drops a key.

import { describe, it, expect } from "vitest";

import en from "../../../../messages/en.json";
import ko from "../../../../messages/ko.json";

// Stable list — kept in sync with `crate::collaboration::ErrorCode`.
// Each rename / addition on the server side has to land here too;
// the test below ensures nothing else is required to keep the FE
// catalogue honest.
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

describe("collaboration ErrorCode i18n coverage", () => {
  it("every ErrorCode has an en message", () => {
    const messages = (en as { collaboration: { errors: Record<string, string> } }).collaboration.errors;
    for (const code of ERROR_CODES) {
      expect(messages[code], `missing en key collaboration.errors.${code}`).toBeTypeOf(
        "string",
      );
      expect(messages[code]?.length, `empty en key collaboration.errors.${code}`).toBeGreaterThan(0);
    }
  });

  it("every ErrorCode has a ko message", () => {
    const messages = (ko as { collaboration: { errors: Record<string, string> } }).collaboration.errors;
    for (const code of ERROR_CODES) {
      expect(messages[code], `missing ko key collaboration.errors.${code}`).toBeTypeOf(
        "string",
      );
      expect(messages[code]?.length, `empty ko key collaboration.errors.${code}`).toBeGreaterThan(0);
    }
  });
});

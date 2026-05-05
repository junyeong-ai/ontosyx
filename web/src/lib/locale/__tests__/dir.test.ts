import { describe, expect, it } from "vitest";
import { directionForLocale } from "../dir";

describe("directionForLocale", () => {
  it("returns ltr for the shipped en + ko bundles", () => {
    expect(directionForLocale("en")).toBe("ltr");
    expect(directionForLocale("ko")).toBe("ltr");
    expect(directionForLocale("en-US")).toBe("ltr");
    expect(directionForLocale("ko-KR")).toBe("ltr");
  });

  it("returns rtl for ar / he / fa / ur", () => {
    expect(directionForLocale("ar")).toBe("rtl");
    expect(directionForLocale("he")).toBe("rtl");
    expect(directionForLocale("fa")).toBe("rtl");
    expect(directionForLocale("ur")).toBe("rtl");
  });

  it("falls back to the base subtag", () => {
    expect(directionForLocale("ar-EG")).toBe("rtl");
    expect(directionForLocale("ar-SA")).toBe("rtl");
    expect(directionForLocale("he-IL")).toBe("rtl");
    expect(directionForLocale("fa-IR")).toBe("rtl");
  });

  it("falls back to ltr for unknown tags", () => {
    expect(directionForLocale("zz")).toBe("ltr");
    expect(directionForLocale("klingon-US")).toBe("ltr");
    expect(directionForLocale("")).toBe("ltr");
  });
});

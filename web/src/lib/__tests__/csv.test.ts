import { describe, it, expect } from "vitest";
import { toCsv } from "../csv";

describe("toCsv", () => {
  it("renders a simple header + row with CRLF terminators", () => {
    const out = toCsv(["a", "b"], [["1", "2"]]);
    expect(out).toBe("a,b\r\n1,2\r\n");
  });

  it("quotes fields containing commas", () => {
    const out = toCsv(["x"], [["foo, bar"]]);
    expect(out).toBe("x\r\n\"foo, bar\"\r\n");
  });

  it("quotes fields containing newlines", () => {
    const out = toCsv(["x"], [["line1\nline2"]]);
    expect(out).toBe('x\r\n"line1\nline2"\r\n');
  });

  it("doubles embedded double quotes inside quoted fields", () => {
    const out = toCsv(["x"], [['he said "hi"']]);
    expect(out).toBe('x\r\n"he said ""hi"""\r\n');
  });

  it("renders numbers via String() coercion", () => {
    const out = toCsv(["pi"], [[3.14]]);
    expect(out).toBe("pi\r\n3.14\r\n");
  });

  it("emits header-only when rows is empty", () => {
    const out = toCsv(["a", "b"], []);
    expect(out).toBe("a,b\r\n");
  });

  it("preserves Korean text without escaping", () => {
    // Korean chars don't trigger the quote-required regex,
    // so they stream through verbatim — UTF-8 in / UTF-8 out.
    const out = toCsv(["질문"], [["월간 활성 사용자"]]);
    expect(out).toBe("질문\r\n월간 활성 사용자\r\n");
  });
});

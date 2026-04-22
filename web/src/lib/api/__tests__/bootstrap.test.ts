import { describe, it, expect } from "vitest";

import { parseGlossaryDraft } from "@/lib/api/bootstrap";

describe("parseGlossaryDraft", () => {
  it("returns empty array for empty / whitespace input", () => {
    expect(parseGlossaryDraft("")).toEqual([]);
    expect(parseGlossaryDraft("   \n\t  ")).toEqual([]);
  });

  it("treats each non-empty line as a bare term by default", () => {
    const out = parseGlossaryDraft("Customer\nOrder\nAccount\n");
    expect(out).toEqual([
      { term: "Customer", description: undefined, aliases: [] },
      { term: "Order", description: undefined, aliases: [] },
      { term: "Account", description: undefined, aliases: [] },
    ]);
  });

  it("parses `term: description`", () => {
    const out = parseGlossaryDraft("Customer: a buyer of goods");
    expect(out).toEqual([
      {
        term: "Customer",
        description: "a buyer of goods",
        aliases: [],
      },
    ]);
  });

  it("parses `term | aliases`", () => {
    const out = parseGlossaryDraft("Customer | client, buyer, account-holder");
    expect(out).toEqual([
      {
        term: "Customer",
        description: undefined,
        aliases: ["client", "buyer", "account-holder"],
      },
    ]);
  });

  it("parses combined `term: description | aliases`", () => {
    const out = parseGlossaryDraft(
      "Customer: a buyer of goods | client, buyer",
    );
    expect(out).toEqual([
      {
        term: "Customer",
        description: "a buyer of goods",
        aliases: ["client", "buyer"],
      },
    ]);
  });

  it("drops rows whose term portion is whitespace only", () => {
    const out = parseGlossaryDraft("  : detail only | alias\nActual");
    expect(out).toEqual([
      { term: "Actual", description: undefined, aliases: [] },
    ]);
  });

  it("filters blank aliases from the pipe list", () => {
    const out = parseGlossaryDraft("Sku | , a1, , a2 ,  ");
    expect(out).toEqual([
      {
        term: "Sku",
        description: undefined,
        aliases: ["a1", "a2"],
      },
    ]);
  });

  it("ignores blank description portions", () => {
    const out = parseGlossaryDraft("Term:    ");
    expect(out).toEqual([
      { term: "Term", description: undefined, aliases: [] },
    ]);
  });
});

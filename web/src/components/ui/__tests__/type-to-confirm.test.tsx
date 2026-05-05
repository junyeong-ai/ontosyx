import { describe, expect, it, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";

import {
  TypeToConfirmField,
  matchesConfirmPhrase,
} from "../type-to-confirm";

describe("TypeToConfirmField", () => {
  it("renders the label and phrase", () => {
    render(
      <TypeToConfirmField
        phrase="my-project"
        value=""
        onChange={() => {}}
        label="Project name"
      />,
    );
    expect(screen.getByText("Project name")).toBeTruthy();
    expect(screen.getByText("(my-project)")).toBeTruthy();
  });

  it("hides the phrase echo when `showPhrase` is false", () => {
    render(
      <TypeToConfirmField
        phrase="my-project"
        value=""
        onChange={() => {}}
        label="Project name"
        showPhrase={false}
      />,
    );
    expect(screen.queryByText("(my-project)")).toBeNull();
  });

  it("renders an input wired to value+onChange", () => {
    const onChange = vi.fn();
    const { container } = render(
      <TypeToConfirmField
        phrase="x"
        value="abc"
        onChange={onChange}
        label="Field"
      />,
    );
    const input = container.querySelector("input")!;
    expect(input.value).toBe("abc");
    fireEvent.change(input, { target: { value: "abcd" } });
    expect(onChange).toHaveBeenCalledWith("abcd");
  });

  it("disables spellcheck + autocomplete to keep the gate trustworthy", () => {
    const { container } = render(
      <TypeToConfirmField
        phrase="x"
        value=""
        onChange={() => {}}
        label="Field"
      />,
    );
    const input = container.querySelector("input")!;
    expect(input.getAttribute("spellcheck")).toBe("false");
    expect(input.getAttribute("autocomplete")).toBe("off");
  });

  it("renders the hint and wires it via aria-describedby", () => {
    const { container } = render(
      <TypeToConfirmField
        phrase="x"
        value=""
        onChange={() => {}}
        label="Field"
        hint="Type the value verbatim — case-sensitive."
      />,
    );
    const input = container.querySelector("input")!;
    const describedBy = input.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    expect(container.querySelector(`#${describedBy}`)?.textContent).toContain(
      "case-sensitive",
    );
  });
});

describe("matchesConfirmPhrase", () => {
  it("requires exact match (case-sensitive)", () => {
    expect(matchesConfirmPhrase("ontosyx", "ontosyx")).toBe(true);
    expect(matchesConfirmPhrase("Ontosyx", "ontosyx")).toBe(false);
    expect(matchesConfirmPhrase("ontosy", "ontosyx")).toBe(false);
    expect(matchesConfirmPhrase("ontosyx ", "ontosyx")).toBe(false);
  });
});

import { describe, expect, it } from "vitest";

import { hasInlineMarkup, parseInline } from "./inlineMarkdown";

const text = (t: string) => ({ kind: "text", text: t });

/**
 * This runs on every blurred block, so its failures are visible everywhere at
 * once. The cases that matter are the ones where formatting must NOT happen —
 * a parser that eats a stray asterisk in "2 * 3" makes ordinary writing feel
 * broken.
 */
describe("inline markdown", () => {
  it("reads the ordinary marks", () => {
    expect(parseInline("**enzyme**")).toEqual([
      { kind: "bold", text: "enzyme" },
    ]);
    expect(parseInline("*slow*")).toEqual([{ kind: "italic", text: "slow" }]);
    expect(parseInline("`pH`")).toEqual([{ kind: "code", text: "pH" }]);
    expect(parseInline("~~wrong~~")).toEqual([
      { kind: "strike", text: "wrong" },
    ]);
    expect(parseInline("==exam==")).toEqual([
      { kind: "highlight", text: "exam" },
    ]);
    expect(parseInline("_slow_")).toEqual([{ kind: "italic", text: "slow" }]);
    expect(parseInline("__bold__")).toEqual([{ kind: "bold", text: "bold" }]);
  });

  it("tries the longest marker first", () => {
    // `**x**` read as italic-then-stray-asterisks is the classic bug here.
    expect(parseInline("***both***")).toEqual([
      { kind: "boldItalic", text: "both" },
    ]);
    expect(parseInline("**bold**")).toEqual([{ kind: "bold", text: "bold" }]);
  });

  it("keeps the surrounding text", () => {
    expect(parseInline("An **enzyme** is a catalyst.")).toEqual([
      text("An "),
      { kind: "bold", text: "enzyme" },
      text(" is a catalyst."),
    ]);
  });

  /** The failure that would make the editor feel broken during ordinary use. */
  it("leaves unmatched markers as literal text", () => {
    expect(parseInline("2 * 3 * 4")).toEqual([text("2 * 3 * 4")]);
    expect(parseInline("snake_case_name")).toEqual([text("snake_case_name")]);
    // Mid-typing: the closing pair doesn't exist yet.
    expect(parseInline("this is **not closed")).toEqual([
      text("this is **not closed"),
    ]);
    expect(parseInline("")).toEqual([]);
  });

  /** Backticks suspend the other rules — that's what code formatting is for. */
  it("does not format inside code", () => {
    expect(parseInline("`**literal**`")).toEqual([
      { kind: "code", text: "**literal**" },
    ]);
  });

  it("reads a link, and uses the URL when there's no label", () => {
    expect(parseInline("[VCAA](https://vcaa.vic.edu.au)")).toEqual([
      { kind: "link", text: "VCAA", href: "https://vcaa.vic.edu.au" },
    ]);
    expect(parseInline("[](https://x.com)")).toEqual([
      { kind: "link", text: "https://x.com", href: "https://x.com" },
    ]);
  });

  /**
   * A note can hold anything you paste, so the link pattern is the one place
   * arbitrary text could become something the OS acts on.
   */
  it("only accepts http and https links", () => {
    for (const bad of [
      "[click](javascript:alert(1))",
      "[open](file:///etc/passwd)",
      "[go](vnd.ms-word:ofe|u|file:///tmp/x)",
    ]) {
      const spans = parseInline(bad);
      expect(spans.every((s) => s.kind !== "link")).toBe(true);
      // And it survives as readable text rather than vanishing.
      expect(spans.map((s) => s.text).join("")).toBe(bad);
    }
  });

  it("handles several marks in one line", () => {
    expect(parseInline("**A** and *B* and `C`")).toEqual([
      { kind: "bold", text: "A" },
      text(" and "),
      { kind: "italic", text: "B" },
      text(" and "),
      { kind: "code", text: "C" },
    ]);
  });
});

describe("hasInlineMarkup", () => {
  it("is true only when there is something to parse", () => {
    expect(hasInlineMarkup("plain sentence")).toBe(false);
    expect(hasInlineMarkup("**bold**")).toBe(true);
    expect(hasInlineMarkup("[a](https://b.com)")).toBe(true);
  });
});

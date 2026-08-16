import { describe, expect, it } from "vitest";

import {
  exitsListOnEmptyEnter,
  filterSlash,
  kindAfterEnter,
  markdownShortcut,
} from "./blockShortcuts";

/**
 * These rules are the difference between a block editor and a textarea, and
 * they're the part that misfires in ordinary writing — a shortcut that triggers
 * on `#hashtag` or `5. something` mid-sentence makes the editor unusable.
 */
describe("markdown shortcuts", () => {
  it("turns a marker into its block and removes the marker", () => {
    expect(markdownShortcut("# Enzymes")).toEqual({
      kind: "h1",
      text: "Enzymes",
    });
    expect(markdownShortcut("## Structure")).toEqual({
      kind: "h2",
      text: "Structure",
    });
    expect(markdownShortcut("### Detail")).toEqual({
      kind: "h3",
      text: "Detail",
    });
    expect(markdownShortcut("- A point")).toEqual({
      kind: "bullet",
      text: "A point",
    });
    expect(markdownShortcut("* A point")).toEqual({
      kind: "bullet",
      text: "A point",
    });
    expect(markdownShortcut("1. First")).toEqual({
      kind: "numbered",
      text: "First",
    });
    expect(markdownShortcut("1) First")).toEqual({
      kind: "numbered",
      text: "First",
    });
    expect(markdownShortcut("> Quoted")).toEqual({
      kind: "quote",
      text: "Quoted",
    });
  });

  it("matches the longest marker first", () => {
    // `###` must not be read as `#` followed by `##`.
    expect(markdownShortcut("### x")?.kind).toBe("h3");
    expect(markdownShortcut("## x")?.kind).toBe("h2");
  });

  it("accepts both bracket forms of a checkbox", () => {
    expect(markdownShortcut("[] Do it")).toEqual({
      kind: "todo",
      text: "Do it",
    });
    expect(markdownShortcut("[ ] Do it")).toEqual({
      kind: "todo",
      text: "Do it",
    });
  });

  /** The trailing space is what stops ordinary sentences from transforming. */
  it("requires the space, so real writing is unaffected", () => {
    expect(markdownShortcut("#hashtag")).toBeNull();
    expect(markdownShortcut("-dash")).toBeNull();
    expect(markdownShortcut(">arrow")).toBeNull();
    expect(markdownShortcut("1.5 grams")).toBeNull();
  });

  it("ignores a marker that isn't at the start", () => {
    expect(markdownShortcut("see # 4 below")).toBeNull();
    expect(markdownShortcut("a - b")).toBeNull();
  });

  it("leaves ordinary text alone", () => {
    expect(markdownShortcut("")).toBeNull();
    expect(markdownShortcut("Enzymes lower activation energy.")).toBeNull();
  });

  it("makes a divider from a rule on its own", () => {
    expect(markdownShortcut("---")).toEqual({ kind: "divider", text: "" });
    // Not a divider mid-sentence.
    expect(markdownShortcut("--- and then")).toBeNull();
  });
});

describe("slash menu", () => {
  it("offers everything on a bare slash", () => {
    expect(filterSlash("").length).toBeGreaterThan(8);
  });

  it("matches on a prefix rather than anywhere in the label", () => {
    const kinds = filterSlash("head").map((i) => i.kind);
    expect(kinds).toEqual(["h1", "h2", "h3"]);
    // Substring matching would drag in every label containing "o".
    expect(filterSlash("o").map((i) => i.kind)).not.toContain("h1");
  });

  it("finds an item by a word inside its label", () => {
    expect(filterSlash("list").map((i) => i.kind)).toEqual([
      "bullet",
      "numbered",
    ]);
  });

  it("finds an item by a keyword that isn't in its label", () => {
    expect(filterSlash("tick").map((i) => i.kind)).toEqual(["todo"]);
    expect(filterSlash("screen").map((i) => i.kind)).toEqual(["image"]);
  });

  it("returns nothing rather than everything when nothing matches", () => {
    // Falling back to the full list would make the menu look broken.
    expect(filterSlash("zzz")).toEqual([]);
  });
});

describe("pressing enter", () => {
  it("continues a list but not a heading", () => {
    expect(kindAfterEnter("bullet")).toBe("bullet");
    expect(kindAfterEnter("numbered")).toBe("numbered");
    expect(kindAfterEnter("todo")).toBe("todo");
    // The line after a heading is almost never another heading.
    expect(kindAfterEnter("h1")).toBe("paragraph");
    expect(kindAfterEnter("quote")).toBe("paragraph");
  });

  /** Enter twice is the only way out of a list without the mouse. */
  it("leaves a list when the item is empty", () => {
    expect(exitsListOnEmptyEnter("bullet", "")).toBe(true);
    expect(exitsListOnEmptyEnter("todo", "")).toBe(true);
    expect(exitsListOnEmptyEnter("bullet", "still typing")).toBe(false);
    expect(exitsListOnEmptyEnter("paragraph", "")).toBe(false);
  });
});

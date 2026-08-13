import { describe, expect, it } from "vitest";

import { parseCloze } from "./CardFace";

/**
 * The cloze renderer is the one piece of frontend logic that can leak an answer.
 * These pin the parse; the component then chooses whether each chunk is shown.
 */
describe("parseCloze", () => {
  it("splits a single deletion from its surrounding text", () => {
    expect(parseCloze("The powerhouse is the {{c1::mitochondrion}}.")).toEqual([
      { kind: "text", text: "The powerhouse is the " },
      { kind: "cloze", index: 1, text: "mitochondrion", hint: undefined },
      { kind: "text", text: "." },
    ]);
  });

  it("keeps distinct indices separate", () => {
    const out = parseCloze("{{c1::Transcription}} happens in the {{c2::nucleus}}");
    const clozes = out.filter((c) => c.kind === "cloze");
    expect(clozes.map((c) => (c.kind === "cloze" ? c.index : 0))).toEqual([1, 2]);
  });

  it("handles adjacent deletions without swallowing the one between", () => {
    const out = parseCloze("{{c1::a}}{{c2::b}}");
    expect(out).toHaveLength(2);
    expect(out.every((c) => c.kind === "cloze")).toBe(true);
  });

  it("captures Anki's hint form", () => {
    const out = parseCloze("Capital of France is {{c1::Paris::city}}");
    const cloze = out.find((c) => c.kind === "cloze");
    expect(cloze).toMatchObject({ index: 1, text: "Paris", hint: "city" });
  });

  it("leaves text with no deletions untouched", () => {
    expect(parseCloze("just a sentence")).toEqual([
      { kind: "text", text: "just a sentence" },
    ]);
  });

  it("leaves an unterminated deletion as plain text", () => {
    const out = parseCloze("{{c1::never closed");
    expect(out.filter((c) => c.kind === "cloze")).toHaveLength(0);
    expect(out).toEqual([{ kind: "text", text: "{{c1::never closed" }]);
  });

  it("ignores a marker with no cloze number", () => {
    const out = parseCloze("{{c::nothing}}");
    expect(out.filter((c) => c.kind === "cloze")).toHaveLength(0);
  });

  /**
   * Malformed-but-closed input (a nested-looking marker) does parse to
   * *something* — the non-greedy match treats the inner text as the answer and
   * the tail as a hint. That is acceptable for input that was already broken;
   * what matters is that it terminates, doesn't throw, and still yields a chunk
   * list the renderer can omit from.
   */
  it("does not throw on nested-looking markers", () => {
    expect(() => parseCloze("{{c1::a {{c2::b}} c}}")).not.toThrow();
    expect(parseCloze("{{c1::a {{c2::b}} c}}").length).toBeGreaterThan(0);
  });

  it("preserves multi-byte characters inside a deletion", () => {
    const out = parseCloze("β-oxidation yields {{c1::acetyl-CoA — 2 carbons}}");
    const cloze = out.find((c) => c.kind === "cloze");
    expect(cloze).toMatchObject({ text: "acetyl-CoA — 2 carbons" });
  });

  /**
   * The security-ish property: for the card being shown, the deleted text must
   * be a separate chunk the component can omit entirely — not embedded in a
   * text run it would have to hide with CSS.
   */
  it("isolates the answer so it can be omitted from the DOM", () => {
    const out = parseCloze("The answer is {{c1::SECRET}}.");
    const visible = out
      .filter((c) => !(c.kind === "cloze" && c.index === 1))
      .map((c) => (c.kind === "text" ? c.text : ""))
      .join("");
    expect(visible).not.toContain("SECRET");
  });
});

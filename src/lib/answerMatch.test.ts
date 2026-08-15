import { describe, expect, it } from "vitest";

import { editDistance, hintFor, judge, missingWords, normalise } from "./answerMatch";

describe("normalise", () => {
  it("strips case, punctuation and articles", () => {
    expect(normalise("The Mitochondrion!")).toBe("mitochondrion");
    expect(normalise("  ATP,  ADP  ")).toBe("atp adp");
  });

  it("keeps negation words, which change the meaning", () => {
    expect(normalise("does not increase")).toContain("not");
  });

  it("unwraps cloze markers and markdown", () => {
    expect(normalise("the {{c1::nucleus}}")).toBe("nucleus");
    expect(normalise("**active** transport")).toBe("active transport");
  });
});

describe("judge — short answers", () => {
  it("accepts an exact answer", () => {
    expect(judge("mitochondria", "mitochondria")).toBe("correct");
  });

  it("forgives case, punctuation and an article", () => {
    expect(judge("The Mitochondria.", "mitochondria")).toBe("correct");
  });

  it("forgives a small spelling slip", () => {
    expect(judge("mitochondia", "mitochondria")).toBe("correct");
  });

  it("calls a bigger slip close rather than correct", () => {
    expect(judge("mitokondrea", "mitochondria")).toBe("close");
  });

  it("rejects a different term", () => {
    expect(judge("ribosome", "mitochondria")).toBe("different");
  });

  it("rejects an empty answer", () => {
    expect(judge("   ", "mitochondria")).toBe("different");
  });
});

describe("judge — longer answers", () => {
  const expected = "enzymes lower the activation energy required for a reaction";

  it("accepts an answer carrying the same content", () => {
    expect(judge("enzymes lower activation energy required for reaction", expected)).toBe("correct");
  });

  it("calls a partial answer close", () => {
    expect(judge("enzymes lower energy", expected)).toBe("close");
  });

  it("rejects an unrelated answer", () => {
    expect(judge("the cell membrane is semi permeable", expected)).toBe("different");
  });
});

/**
 * The failure that matters most: an answer disagreeing about whether something
 * happens shares nearly every word with the right one. Calling that "close"
 * would actively teach the wrong thing.
 */
describe("negation", () => {
  it("never calls a negated answer close to a positive one", () => {
    expect(judge("enzymes do not lower activation energy", "enzymes lower activation energy")).toBe(
      "different",
    );
    expect(judge("it increases", "it does not increase")).toBe("different");
  });

  it("still accepts a correctly negated answer", () => {
    expect(judge("does not increase", "does not increase")).toBe("correct");
  });
});

describe("editDistance", () => {
  it("measures simple edits", () => {
    expect(editDistance("cat", "cat")).toBe(0);
    expect(editDistance("cat", "cot")).toBe(1);
    expect(editDistance("cat", "cats")).toBe(1);
  });

  it("bails out rather than working through a long comparison", () => {
    expect(editDistance("a".repeat(200), "b".repeat(200), 5)).toBeGreaterThan(5);
  });
});

describe("missingWords", () => {
  it("names what was left out", () => {
    const missing = missingWords("enzymes lower energy", "enzymes lower the activation energy");
    expect(missing).toContain("activation");
    expect(missing).not.toContain("enzymes");
  });
});

describe("hintFor", () => {
  const answer = "active transport";

  it("gives the shape at level one without giving the answer", () => {
    const hint = hintFor(answer, 1);
    expect(hint).toMatch(/^a·+ t·+$/);
    expect(hint).not.toContain("active");
  });

  it("opens the first word at level two", () => {
    expect(hintFor(answer, 2)).toMatch(/^active t·+$/);
  });

  it("opens half at level three", () => {
    expect(hintFor("the sodium potassium pump maintains", 3)).toContain("sodium");
  });

  it("handles an empty answer", () => {
    expect(hintFor("", 1)).toBe("");
  });
});

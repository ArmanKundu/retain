import { describe, expect, it } from "vitest";

import { isTableDivider, isTableRow, tableCells } from "./Markdown";

/**
 * Tables were added because the notes prompt now asks for one whenever two
 * things are compared. Without the parser the pipes rendered as literal text,
 * so a prompt change alone would have made generated notes *worse* — these pin
 * the three predicates that decide whether a block is a table at all.
 */
describe("table detection", () => {
  it("accepts a row with pipes at both ends and a divider inside", () => {
    expect(isTableRow("| Enzyme | Substrate |")).toBe(true);
    expect(isTableRow("|a|b|")).toBe(true);
  });

  it("rejects prose that merely contains a pipe", () => {
    // The common false positive: a sentence about a logical OR, or a file path.
    expect(isTableRow("The operator is | in most languages")).toBe(false);
    expect(isTableRow("| only one edge")).toBe(false);
    expect(isTableRow("|no inner pipe|")).toBe(false);
    expect(isTableRow("")).toBe(false);
  });

  /** The divider is the whole reason a header row isn't just a paragraph. */
  it("recognises a divider in each alignment form", () => {
    expect(isTableDivider("|---|---|")).toBe(true);
    expect(isTableDivider("| --- | --- |")).toBe(true);
    expect(isTableDivider("|:---|---:|")).toBe(true);
    expect(isTableDivider("|:--:|:--:|")).toBe(true);
  });

  it("does not mistake a content row for a divider", () => {
    expect(isTableDivider("| Enzyme | Substrate |")).toBe(false);
    // Digits are content, however dash-like the row looks.
    expect(isTableDivider("| 1-2 | 3-4 |")).toBe(false);
  });

  it("splits cells and trims them", () => {
    expect(tableCells("| Enzyme |  Substrate  |")).toEqual([
      "Enzyme",
      "Substrate",
    ]);
  });

  it("keeps empty cells so a row still lines up with its header", () => {
    // Dropping empties would shift every later cell one column left, which is
    // worse than a blank box.
    expect(tableCells("| a |  | c |")).toEqual(["a", "", "c"]);
  });
});

// Markdown shortcuts, and the slash menu.
//
// Separated from the editor component because this is the part with rules in
// it, and rules are worth testing. The component only decides where the cursor
// goes afterwards.
//
// The behaviour being copied: you type `## ` and the block *becomes* a heading,
// with the `## ` gone. It never appears as literal text. That transformation is
// the whole feel of a block editor — get it wrong and you have a textarea.

import type { NoteBlockKind } from "./types";

/** What the block turns into, and what's left of the text. */
export interface Transform {
  kind: NoteBlockKind;
  text: string;
}

/**
 * A leading marker typed at the start of a block.
 *
 * Ordered longest-first so `###` is matched before `##`, and `1. ` before `- `
 * can't shadow anything. Every pattern requires the trailing space: typing
 * `#hashtag` is not a heading, and a shortcut that fires without the space
 * makes ordinary sentences unwritable.
 */
const MARKERS: [RegExp, NoteBlockKind][] = [
  [/^### (.*)$/, "h3"],
  [/^## (.*)$/, "h2"],
  [/^# (.*)$/, "h1"],
  [/^> (.*)$/, "quote"],
  [/^```(.*)$/, "code"],
  [/^(?:---|\*\*\*)$/, "divider"],
  // Both bracket forms, because both get typed.
  [/^\[\] (.*)$/, "todo"],
  [/^\[ \] (.*)$/, "todo"],
  [/^[-*] (.*)$/, "bullet"],
  [/^\d+[.)] (.*)$/, "numbered"],
];

/**
 * Whether what was just typed turns this block into something else.
 *
 * Returns `null` when nothing matches, which is the overwhelmingly common case
 * and has to stay cheap — this runs on every keystroke.
 */
export function markdownShortcut(text: string): Transform | null {
  for (const [pattern, kind] of MARKERS) {
    const match = pattern.exec(text);
    if (match) return { kind, text: match[1] ?? "" };
  }
  return null;
}

export interface SlashItem {
  kind: NoteBlockKind;
  label: string;
  hint: string;
  /** Extra words that should find this item, beyond its label. */
  keywords: string[];
}

export const SLASH_ITEMS: SlashItem[] = [
  {
    kind: "paragraph",
    label: "Text",
    hint: "Plain paragraph",
    keywords: ["plain", "body"],
  },
  {
    kind: "h1",
    label: "Heading 1",
    hint: "Big section",
    keywords: ["title", "h1"],
  },
  { kind: "h2", label: "Heading 2", hint: "Sub-section", keywords: ["h2"] },
  { kind: "h3", label: "Heading 3", hint: "Small heading", keywords: ["h3"] },
  {
    kind: "bullet",
    label: "Bulleted list",
    hint: "One idea per line",
    keywords: ["ul", "point"],
  },
  {
    kind: "numbered",
    label: "Numbered list",
    hint: "Steps in order",
    keywords: ["ol", "step"],
  },
  {
    kind: "todo",
    label: "To-do",
    hint: "A checkbox",
    keywords: ["task", "check", "tick"],
  },
  {
    kind: "quote",
    label: "Quote",
    hint: "Set off from the text",
    keywords: ["blockquote", "cite"],
  },
  {
    kind: "code",
    label: "Code",
    hint: "Monospaced",
    keywords: ["mono", "pre"],
  },
  {
    kind: "divider",
    label: "Divider",
    hint: "A section break",
    keywords: ["hr", "line", "rule"],
  },
  {
    kind: "image",
    label: "Screenshot",
    hint: "Capture your screen",
    keywords: ["picture", "screen", "img"],
  },
];

/**
 * Filter the slash menu.
 *
 * Matched on a prefix rather than a substring: typing `/h` should offer the
 * headings, not every item containing an "h". Keywords are matched the same
 * way, so `/tick` finds the to-do without "tick" being in its label.
 */
export function filterSlash(query: string): SlashItem[] {
  const q = query.trim().toLowerCase();
  if (q === "") return SLASH_ITEMS;

  return SLASH_ITEMS.filter(
    (item) =>
      item.label.toLowerCase().startsWith(q) ||
      item.keywords.some((k) => k.startsWith(q)) ||
      // A word inside a multi-word label still counts: `/list` should find
      // both list types even though neither label starts with it.
      item.label
        .toLowerCase()
        .split(" ")
        .some((w) => w.startsWith(q)),
  );
}

/**
 * What pressing Enter at the end of this block should produce.
 *
 * A list continues itself — that's the point of a list — but a heading does
 * not, because the line after a heading is almost never another heading.
 */
export function kindAfterEnter(kind: NoteBlockKind): NoteBlockKind {
  return kind === "bullet" || kind === "numbered" || kind === "todo"
    ? kind
    : "paragraph";
}

/**
 * Whether Enter on an empty block of this kind should end the list instead of
 * adding another item.
 *
 * This is the standard way out of a list: press Enter twice. Without it the
 * only escape from a bulleted list is the mouse.
 */
export function exitsListOnEmptyEnter(
  kind: NoteBlockKind,
  text: string,
): boolean {
  return (
    text === "" && (kind === "bullet" || kind === "numbered" || kind === "todo")
  );
}

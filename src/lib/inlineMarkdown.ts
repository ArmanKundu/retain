// Inline formatting inside a block.
//
// Last release this was the stated limitation: a textarea per block gets you
// correct undo and pasting, and costs you bold-three-words-inside-a-paragraph.
// That was true of a textarea *alone*. It isn't true of a textarea that's only
// mounted while the cursor is in it.
//
// So each block renders as formatted HTML when it's not focused, and swaps to
// the raw textarea the moment you click into it. You see `**enzyme**` while
// you're editing that line and **enzyme** everywhere else — which is how
// Obsidian's live preview works, and it needs no contenteditable at all.
//
// This file is the parser. It runs on every blurred block on every render, so
// it is a single left-to-right pass with no backtracking.

export type Span =
  | { kind: "text"; text: string }
  | { kind: "bold"; text: string }
  | { kind: "italic"; text: string }
  | { kind: "boldItalic"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strike"; text: string }
  | { kind: "highlight"; text: string }
  | { kind: "link"; text: string; href: string };

/**
 * Build a delimiter pattern that won't fire on arithmetic.
 *
 * The content must begin and end with a non-space. This is CommonMark's
 * flanking rule in its useful form, and without it `2 * 3 * 4` renders as
 * "2 _3_ 4" — which is the failure that makes an editor feel broken during
 * ordinary writing, because nobody expects a maths line to become italic.
 */
function delimited(mark: string, inner: string): RegExp {
  const m = mark.replace(/[*~=_`]/g, "\\$&");
  // non-space, then anything, ending in non-space — or a single non-space.
  return new RegExp(`^${m}([^${inner}\\s](?:[^${inner}]*[^${inner}\\s])?)${m}`);
}

interface Rule {
  pattern: RegExp;
  kind: Span["kind"];
  /**
   * Whether this marker may open in the middle of a word.
   *
   * False for underscores, which is why `snake_case_name` survives in real
   * Markdown and is the reason CommonMark treats `_` and `*` differently. An
   * identifier turning half-italic is not a formatting choice anyone made.
   */
  intraword: boolean;
}

/**
 * Ordered longest-marker-first.
 *
 * `***` has to be tried before `**`, and `**` before `*`, or bold text reads as
 * an italic containing a stray asterisk. Code is matched before everything else
 * because backticks suspend the other rules — `` `**not bold**` `` is literal,
 * which is the whole point of code formatting.
 */
const RULES: Rule[] = [
  // Code first, and deliberately permissive about spaces: `` ` a ` `` is
  // legitimate, and inside it nothing else applies.
  { pattern: /^`([^`]+)`/, kind: "code", intraword: true },
  { pattern: delimited("***", "*"), kind: "boldItalic", intraword: true },
  { pattern: delimited("**", "*"), kind: "bold", intraword: true },
  { pattern: delimited("__", "_"), kind: "bold", intraword: false },
  { pattern: delimited("~~", "~"), kind: "strike", intraword: true },
  { pattern: delimited("==", "="), kind: "highlight", intraword: true },
  { pattern: delimited("*", "*"), kind: "italic", intraword: true },
  { pattern: delimited("_", "_"), kind: "italic", intraword: false },
];

/** `[label](https://…)`. Only http(s), so a link can't be a `javascript:` URL. */
const LINK = /^\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)/;

/**
 * Split a line into formatted spans.
 *
 * Unmatched markers stay as literal text rather than being swallowed: a lone
 * `*` in "2 * 3" has to survive, and an unclosed `**` mid-sentence must not
 * turn the rest of the note bold while you're still typing it.
 */
export function parseInline(source: string): Span[] {
  const out: Span[] = [];
  let plain = "";
  let i = 0;

  const flush = () => {
    if (plain) {
      out.push({ kind: "text", text: plain });
      plain = "";
    }
  };

  while (i < source.length) {
    const rest = source.slice(i);

    const link = LINK.exec(rest);
    if (link) {
      flush();
      out.push({ kind: "link", text: link[1] || link[2], href: link[2] });
      i += link[0].length;
      continue;
    }

    // What precedes the marker decides whether an underscore may open at all.
    const before = i === 0 ? "" : source[i - 1];
    const insideWord = /[\p{L}\p{N}]/u.test(before);

    let matched = false;
    for (const rule of RULES) {
      if (!rule.intraword && insideWord) continue;
      const m = rule.pattern.exec(rest);
      if (!m) continue;
      flush();
      out.push({ kind: rule.kind, text: m[1] } as Span);
      i += m[0].length;
      matched = true;
      break;
    }
    if (matched) continue;

    plain += source[i];
    i += 1;
  }

  flush();
  return out;
}

/** Whether a line has any formatting worth rendering. Cheap early-out. */
export function hasInlineMarkup(source: string): boolean {
  return /[*_`~=]|\[[^\]]*\]\(/.test(source);
}

/** `| a | b |` — a row has a pipe at each end and at least one inside. */
export function isTableRow(line: string): boolean {
  return (
    line.startsWith("|") &&
    line.endsWith("|") &&
    line.slice(1, -1).includes("|")
  );
}

/** `|---|:--:|` — the line that makes the row above a header rather than prose. */
export function isTableDivider(line: string): boolean {
  const t = line.trim();
  return isTableRow(t) && /^[|\s:-]+$/.test(t);
}

export function tableCells(line: string): string[] {
  return line
    .slice(1, -1)
    .split("|")
    .map((c) => c.trim());
}

// A small Markdown renderer.
//
// Saved notes were displayed as raw text, so a generated note arrived looking
// like `## Enzymes` and `**activation energy**` — the syntax visible, the
// structure invisible. That made the Library read as a debug view of a text
// field rather than something you'd study from.
//
// Hand-written rather than a dependency, for two reasons. The input is not
// arbitrary web Markdown — it's what the models produce, which is headings,
// lists, bold, code and the occasional table row. And a renderer that accepts
// raw HTML is a script-injection surface for text that arrives over the network;
// this one has no path to `dangerouslySetInnerHTML` at all, so it cannot have
// that bug.

import type { ReactNode } from "react";

import { cx } from "./ui";

/** Inline formatting: `**bold**`, `*italic*`, `` `code` ``. */
function inline(text: string, keyPrefix: string): ReactNode[] {
  const out: ReactNode[] = [];
  // One pass over the three inline forms. Ordered so `**` is tried before `*`.
  const pattern = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*]+\*)/g;

  let last = 0;
  let match: RegExpExecArray | null;
  let i = 0;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) {
      out.push(text.slice(last, match.index));
    }

    const token = match[0];
    const key = `${keyPrefix}-${i++}`;

    if (token.startsWith("**")) {
      out.push(
        <strong key={key} className="font-semibold text-[var(--ink)]">
          {token.slice(2, -2)}
        </strong>,
      );
    } else if (token.startsWith("`")) {
      out.push(
        <code
          key={key}
          className="rounded-[6px] bg-[var(--surface-hi)] px-1.5 py-0.5 font-mono text-[0.9em] text-[var(--ink)]"
        >
          {token.slice(1, -1)}
        </code>,
      );
    } else {
      out.push(
        <em key={key} className="italic">
          {token.slice(1, -1)}
        </em>,
      );
    }

    last = match.index + token.length;
  }

  if (last < text.length) out.push(text.slice(last));
  return out;
}

/**
 * Render Markdown as React elements.
 *
 * Block-level parsing is a single pass over lines, grouping consecutive list
 * items and paragraph lines. Anything unrecognised falls through as a
 * paragraph, so unusual input degrades to readable prose rather than vanishing.
 */
export function Markdown({
  source,
  className,
}: {
  source: string;
  className?: string;
}) {
  const lines = source.replace(/\r/g, "").split("\n");
  const blocks: ReactNode[] = [];

  let paragraph: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;

  const flushParagraph = () => {
    if (paragraph.length === 0) return;
    const text = paragraph.join(" ");
    blocks.push(
      <p
        key={`p-${blocks.length}`}
        className="text-[15px] leading-[1.7] text-[var(--ink-dim)]"
      >
        {inline(text, `p${blocks.length}`)}
      </p>,
    );
    paragraph = [];
  };

  const flushList = () => {
    if (!list) return;
    const { ordered, items } = list;
    const Tag = ordered ? "ol" : "ul";
    blocks.push(
      <Tag
        key={`l-${blocks.length}`}
        className={cx(
          "space-y-1.5 pl-5 text-[15px] leading-[1.7] text-[var(--ink-dim)]",
          ordered ? "list-decimal" : "list-disc",
        )}
      >
        {items.map((item, i) => (
          <li key={i} className="pl-1 marker:text-[var(--ink-faint)]">
            {inline(item, `l${blocks.length}-${i}`)}
          </li>
        ))}
      </Tag>,
    );
    list = null;
  };

  const flushAll = () => {
    flushParagraph();
    flushList();
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trimEnd();
    const trimmed = line.trim();

    if (trimmed === "") {
      flushAll();
      continue;
    }

    // Tables.
    //
    // Added because the notes prompt now asks for one whenever two things are
    // being compared — a comparison written as prose is one you have to
    // re-read. Without this the pipes rendered as literal text, which would
    // have made generated notes worse rather than better.
    //
    // A table is a header row, a `|---|---|` separator, then body rows. The
    // separator is what distinguishes it from a paragraph that happens to
    // contain a pipe.
    if (
      isTableRow(trimmed) &&
      i + 1 < lines.length &&
      isTableDivider(lines[i + 1])
    ) {
      flushAll();

      const header = tableCells(trimmed);
      const rows: string[][] = [];
      let j = i + 2;
      while (j < lines.length && isTableRow(lines[j].trim())) {
        rows.push(tableCells(lines[j].trim()));
        j++;
      }
      i = j - 1;

      blocks.push(
        // Scrolls inside itself rather than widening the page: a wide table in
        // a fixed column otherwise pushes every other block sideways.
        <div key={blocks.length} className="overflow-x-auto">
          <table className="w-full border-collapse text-[13px]">
            <thead>
              <tr>
                {header.map((cell, n) => (
                  <th
                    key={n}
                    className="border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 py-1.5 text-left font-medium text-[var(--ink)]"
                  >
                    {inline(cell, `th-${blocks.length}-${n}`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, n) => (
                <tr key={n}>
                  {/* Padded to the header's width. A ragged row from a model
                      would otherwise collapse the grid. */}
                  {header.map((_, m) => (
                    <td
                      key={m}
                      className="border border-[var(--line)] px-2.5 py-1.5 align-top text-[var(--ink-dim)]"
                    >
                      {inline(row[m] ?? "", `td-${blocks.length}-${n}-${m}`)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }

    // Headings.
    const heading = /^(#{1,6})\s+(.*)$/.exec(trimmed);
    if (heading) {
      flushAll();
      const level = heading[1].length;
      const text = heading[2];
      const size =
        level === 1
          ? "text-[24px] mt-8 first:mt-0"
          : level === 2
            ? "text-[19px] mt-7 first:mt-0"
            : "text-[16px] mt-6 first:mt-0";
      blocks.push(
        <h3
          key={`h-${blocks.length}`}
          className={cx(
            "font-semibold tracking-[-0.015em] text-[var(--ink)]",
            size,
          )}
        >
          {inline(text, `h${blocks.length}`)}
        </h3>,
      );
      continue;
    }

    // A horizontal rule becomes space rather than a line — the documents these
    // come from use `---` as a section break, and a visible rule at that
    // frequency chops the page up.
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      flushAll();
      blocks.push(<div key={`r-${blocks.length}`} className="h-4" />);
      continue;
    }

    // Blockquote.
    const quote = /^>\s?(.*)$/.exec(trimmed);
    if (quote) {
      flushAll();
      blocks.push(
        <blockquote
          key={`q-${blocks.length}`}
          className="border-l-2 border-[var(--accent)]/40 pl-4 text-[14.5px] leading-[1.7] text-[var(--ink-dim)]"
        >
          {inline(quote[1], `q${blocks.length}`)}
        </blockquote>,
      );
      continue;
    }

    // List items, grouped so a run becomes one list.
    const bullet = /^[-*+]\s+(.*)$/.exec(trimmed);
    const numbered = /^\d+[.)]\s+(.*)$/.exec(trimmed);

    if (bullet || numbered) {
      flushParagraph();
      const ordered = !!numbered;
      const text = (bullet ?? numbered)![1];

      if (list && list.ordered !== ordered) flushList();
      list ??= { ordered, items: [] };
      list.items.push(text);
      continue;
    }

    flushList();
    paragraph.push(trimmed);
  }

  flushAll();

  return <div className={cx("space-y-4", className)}>{blocks}</div>;
}

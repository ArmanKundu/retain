import type { ReactNode } from "react";

import type { QueueItem } from "../lib/types";

/**
 * Rendering the three card types, question side and answer side.
 *
 * The one rule this file exists to enforce: **the question side must not
 * contain the answer.** For a cloze that means actually removing the deleted
 * text from the DOM rather than hiding it with CSS — a hidden element is still
 * selectable, still copyable, and still visible to anyone who thinks to look.
 */

type ClozeChunk =
  | { kind: "text"; text: string }
  | { kind: "cloze"; index: number; text: string; hint?: string };

/**
 * Split cloze text into chunks.
 *
 * Handles `{{c1::answer}}` and Anki's hint form `{{c1::answer::hint}}`.
 * Non-greedy so adjacent deletions don't swallow each other.
 */
export function parseCloze(source: string): ClozeChunk[] {
  const pattern = /\{\{c(\d+)::(.*?)(?:::(.*?))?\}\}/g;
  const chunks: ClozeChunk[] = [];
  let cursor = 0;

  for (let m = pattern.exec(source); m !== null; m = pattern.exec(source)) {
    if (m.index > cursor) {
      chunks.push({ kind: "text", text: source.slice(cursor, m.index) });
    }
    chunks.push({
      kind: "cloze",
      index: Number(m[1]),
      text: m[2],
      hint: m[3],
    });
    cursor = m.index + m[0].length;
  }

  if (cursor < source.length) {
    chunks.push({ kind: "text", text: source.slice(cursor) });
  }
  return chunks;
}

function Blank({ hint }: { hint?: string }) {
  return (
    <span className="mx-0.5 inline-block rounded-[7px] bg-[var(--accent)]/18 px-2 py-0.5 align-baseline text-[var(--accent)]">
      {hint ? hint : "[…]"}
    </span>
  );
}

function Revealed({ children }: { children: ReactNode }) {
  return (
    <span className="mx-0.5 inline-block rounded-[7px] bg-[var(--accent)]/18 px-2 py-0.5 align-baseline font-medium text-[var(--accent)]">
      {children}
    </span>
  );
}

/**
 * Render a cloze card.
 *
 * Only the deletion matching THIS card's index is blanked; other deletions in
 * the same note show their text, because they belong to sibling cards and hiding
 * them would make the sentence unreadable.
 */
function Cloze({
  source,
  index,
  revealed,
}: {
  source: string;
  index: number | null;
  revealed: boolean;
}) {
  return (
    <>
      {parseCloze(source).map((chunk, i) => {
        if (chunk.kind === "text") return <span key={i}>{chunk.text}</span>;
        if (chunk.index !== index) return <span key={i}>{chunk.text}</span>;
        // The answer is simply absent from the tree until revealed.
        return revealed ? (
          <Revealed key={i}>{chunk.text}</Revealed>
        ) : (
          <Blank key={i} hint={chunk.hint} />
        );
      })}
    </>
  );
}

const PROMPT = "text-[13px] uppercase tracking-[0.07em] text-[var(--ink-faint)]";

/** The question side. Never contains the answer. */
export function CardQuestion({ card }: { card: QueueItem }) {
  if (card.noteType === "cloze") {
    return (
      <p className="selectable text-[22px] leading-[1.55] tracking-[-0.01em]">
        <Cloze source={card.front} index={card.clozeIndex} revealed={false} />
      </p>
    );
  }

  if (card.noteType === "quote") {
    return (
      <blockquote className="selectable border-l-2 border-[var(--accent)]/40 pl-5 text-[22px] leading-[1.55] tracking-[-0.01em] italic">
        “{card.front}”
      </blockquote>
    );
  }

  return (
    <p className="selectable text-[22px] leading-[1.55] tracking-[-0.01em]">{card.front}</p>
  );
}

/** The answer side, shown only after an explicit reveal. */
export function CardAnswer({ card }: { card: QueueItem }) {
  if (card.noteType === "cloze") {
    return (
      <p className="selectable text-[22px] leading-[1.55] tracking-[-0.01em]">
        <Cloze source={card.front} index={card.clozeIndex} revealed />
      </p>
    );
  }

  if (card.noteType === "quote") {
    return (
      <div className="space-y-4">
        <div>
          <div className={PROMPT}>Source &amp; context</div>
          <p className="selectable mt-1.5 text-[17px] leading-relaxed">{card.back}</p>
        </div>
        {card.extra && (
          <div>
            <div className={PROMPT}>Theme</div>
            <p className="selectable mt-1.5 text-[17px] leading-relaxed">{card.extra}</p>
          </div>
        )}
      </div>
    );
  }

  return (
    <div>
      <div className={PROMPT}>Answer</div>
      <p className="selectable mt-1.5 text-[19px] leading-relaxed">{card.back}</p>
      {card.extra && (
        <p className="selectable mt-3 text-[14px] leading-relaxed text-[var(--ink-dim)]">
          {card.extra}
        </p>
      )}
    </div>
  );
}

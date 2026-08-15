// Judging a typed answer against the card's back.
//
// This is the delicate part of write-in mode, and the reason it's a separate,
// tested file rather than an inline comparison. Two failure modes, and they
// pull in opposite directions:
//
//   * **Too strict** marks "the mitochondrion" wrong against "mitochondria" and
//     you stop trusting the mode within a day.
//   * **Too loose** marks a vague answer right, which is worse — the whole
//     point of writing it out is that recall is harder than recognition, and a
//     grader that accepts anything hands that benefit straight back.
//
// So the verdict is deliberately three-valued. Retain never silently decides
// you were wrong: a near miss is surfaced *as* a near miss and you make the
// call. The FSRS rating always comes from you, never from this file.

export type Verdict = "correct" | "close" | "different";

/** Strip case, punctuation, articles and extra space. */
export function normalise(text: string): string {
  return text
    .toLowerCase()
    // Cloze markers and markdown emphasis are formatting, not content.
    .replace(/\{\{c\d+::(.*?)(::.*?)?\}\}/g, "$1")
    .replace(/[*_`]/g, "")
    .replace(/[^\p{L}\p{N}\s]/gu, " ")
    .split(/\s+/)
    .filter((w) => w.length > 0 && !STOP.has(w))
    .join(" ")
    .trim();
}

/**
 * Words carrying no meaning for a short answer.
 *
 * Deliberately tiny. A long stop list starts eating words that matter —
 * "not" and "no" especially, where dropping them makes an answer mean its
 * opposite, and the grader would mark "does not increase" correct against
 * "does increase".
 */
const STOP = new Set(["a", "an", "the", "is", "are", "was", "were", "of", "to", "and"]);

/** Levenshtein distance, capped so a long comparison can't get expensive. */
export function editDistance(a: string, b: string, cap = 12): number {
  if (a === b) return 0;
  if (Math.abs(a.length - b.length) > cap) return cap + 1;

  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);

  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    let best = i;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      row[j] = Math.min(row[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost);
      best = Math.min(best, row[j]);
    }
    // Every path through this row already exceeds the cap.
    if (best > cap) return cap + 1;
    prev = row;
  }

  return prev[b.length];
}

/**
 * Content words shared between the two answers, as a fraction of the expected.
 *
 * Words of three letters or fewer are excluded from the denominator. They're
 * connectives — "for", "in", "by" — and counting them means a genuinely good
 * partial answer gets marked down for omitting a preposition, which is not
 * what anyone is testing.
 */
function overlap(typed: string, expected: string): number {
  const want = expected.split(" ").filter((w) => w.length > 3);
  if (want.length === 0) return 0;

  const got = new Set(typed.split(" ").filter(Boolean));
  const hit = want.filter((w) => got.has(w)).length;
  return hit / want.length;
}

/**
 * How close a typed answer is to the expected one.
 *
 * The negation check runs first and overrides everything: an answer that
 * disagrees about whether something happens is wrong no matter how many words
 * it shares.
 */
export function judge(typed: string, expected: string): Verdict {
  const a = normalise(typed);
  const b = normalise(expected);

  if (a.length === 0) return "different";
  if (a === b) return "correct";

  // Negation mismatch. "Increases" and "does not increase" overlap almost
  // entirely, and calling that close would be actively misleading.
  if (hasNegation(typed) !== hasNegation(expected)) return "different";

  // Short answers — a term or two — are judged by spelling.
  if (b.split(" ").length <= 3) {
    const distance = editDistance(a, b);
    if (distance <= Math.max(1, Math.floor(b.length * 0.2))) return "correct";
    if (distance <= Math.max(2, Math.floor(b.length * 0.34))) return "close";
    return "different";
  }

  // Longer answers are judged by how much of the expected content is present.
  const share = overlap(a, b);
  if (share >= 0.8) return "correct";
  if (share >= 0.45) return "close";
  return "different";
}

function hasNegation(text: string): boolean {
  return /\b(not|no|never|cannot|can't|doesn't|does not|isn't|won't|without|un)\b/i.test(text);
}

/**
 * Which words of the expected answer are missing from what was typed.
 *
 * Shown after a near miss, because "you didn't say *active transport*" teaches
 * something and "close" alone doesn't.
 */
export function missingWords(typed: string, expected: string): string[] {
  const got = new Set(normalise(typed).split(" ").filter(Boolean));
  return normalise(expected)
    .split(" ")
    .filter((w) => w.length > 3 && !got.has(w))
    .slice(0, 6);
}

/**
 * A progressively revealed hint.
 *
 * `level` 1 gives the shape — first letters and word lengths. Level 2 opens the
 * first word. Level 3 opens half. Each step should cost you something, or the
 * hint becomes the answer and the card stops testing recall.
 */
export function hintFor(answer: string, level: number): string {
  const words = answer.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "";

  if (level <= 1) {
    return words
      .map((w) => (w.length <= 2 ? w : `${w[0]}${"·".repeat(Math.min(w.length - 1, 8))}`))
      .join(" ");
  }

  const reveal = level === 2 ? 1 : Math.ceil(words.length / 2);
  return words
    .map((w, i) =>
      i < reveal ? w : w.length <= 2 ? w : `${w[0]}${"·".repeat(Math.min(w.length - 1, 8))}`,
    )
    .join(" ");
}

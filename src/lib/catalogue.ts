// The VCE subject picker, the colour palette, and the type inference behind the
// "inferred, overridable" type flag in onboarding.

import type { SubjectType } from "./types";

/**
 * Auto-assigned subject colours.
 *
 * Muted on purpose. These are the only hues in an otherwise grey app, so they
 * carry real meaning in the contribution grid and the goal rings — and six
 * saturated colours side by side would read as a dashboard, which the brief
 * explicitly rules out. All six hold up on both dark and light backgrounds.
 */
export const PALETTE = [
  "#5B8DEF", // blue
  "#4BA97B", // green
  "#C9784A", // amber
  "#A971D6", // purple
  "#D65F6E", // rose
  "#4CA6B8", // teal
] as const;

/** Next unused colour, falling back to reuse once all six are taken. */
export function nextColour(taken: string[]): string {
  return PALETTE.find((c) => !taken.includes(c)) ?? PALETTE[taken.length % PALETTE.length];
}

export interface CatalogueEntry {
  name: string;
  type: SubjectType;
}

/** The prefilled picker from the brief, in the order it lists them. */
export const VCE_SUBJECTS: CatalogueEntry[] = [
  { name: "English", type: "english" },
  { name: "EAL", type: "english" },
  { name: "Literature", type: "english" },
  { name: "Maths Methods", type: "maths" },
  { name: "Specialist Maths", type: "maths" },
  { name: "General Maths", type: "maths" },
  { name: "Biology", type: "science" },
  { name: "Chemistry", type: "science" },
  { name: "Physics", type: "science" },
  { name: "Psychology", type: "science" },
  { name: "Accounting", type: "humanities" },
  { name: "Economics", type: "humanities" },
  { name: "Business Management", type: "humanities" },
  { name: "Legal Studies", type: "humanities" },
  { name: "History", type: "humanities" },
];

/**
 * Guess a type flag for a free-text "Other" subject.
 *
 * Only a guess — onboarding shows the result and lets it be changed, because the
 * flag decides which error-log categories and card templates you get later, and
 * a wrong guess is annoying rather than harmless.
 */
export function inferType(name: string): SubjectType {
  const n = name.trim().toLowerCase();
  if (!n) return "humanities";

  const exact = VCE_SUBJECTS.find((s) => s.name.toLowerCase() === n);
  if (exact) return exact.type;

  const matches = (...words: string[]) => words.some((w) => n.includes(w));

  if (matches("math", "further", "specialist", "methods", "calculus", "statistic")) return "maths";
  if (
    matches(
      "biol", "chem", "physic", "psych", "science", "environmental",
      "health", "food", "systems", "computing", "algorithmic",
    )
  )
    return "science";
  if (matches("english", "literature", "eal", "language", "writing")) return "english";

  // Humanities is the sensible fallback: its error categories are the most
  // general of the four, so a wrong guess here does the least damage.
  return "humanities";
}

export const SUBJECT_TYPE_LABELS: Record<SubjectType, string> = {
  science: "Science",
  maths: "Maths",
  english: "English",
  humanities: "Humanities",
};

export const UNIT_LEVEL_LABELS = {
  "1_2": "Units 1 & 2",
  "3_4": "Units 3 & 4",
} as const;

/**
 * What each type flag changes later, shown in onboarding and Settings so the
 * choice isn't arbitrary-looking.
 */
export const SUBJECT_TYPE_BLURB: Record<SubjectType, string> = {
  science: "Error categories for data misreads, command words and mark allocation.",
  maths: "Error categories for sign errors, CAS misuse and skipped working.",
  english: "Quote cards instead of basic cards, and thesis-level error categories.",
  humanities: "Error categories for evidence, command words and mark allocation.",
};

/**
 * Whether a subject gets Retain's Biology 3/4 surfaces.
 *
 * Mirrors `biology::applies_to` in the Rust backend, which stays authoritative:
 * it decides which error categories the log offers. This copy exists so the
 * sidebar and the Biology screen can decide without a round trip per render.
 * If you change one, change both — they are asserted against each other by the
 * `biology_categories_apply_only_to_biology_three_four` test on the Rust side.
 */
export function isBiologyThreeFour(subject: { name: string; unitLevel: string }): boolean {
  return subject.name.trim().toLowerCase() === "biology" && subject.unitLevel === "3_4";
}

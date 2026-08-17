// How you're tested on a card.
//
// Flip alone is recognition: you look, you decide you knew it, you move on —
// and you're often wrong about having known it. The other modes exist because
// recall is harder than recognition and that difficulty is the point.
//
//   Flip     see it, judge yourself. Fast, good for volume.
//   Write    type the answer. The honest one, and the slowest.
//   Hint     progressive reveal, for when you're stuck but nearly there.
//
// One rule across all of them: **the mode never rates the card**. Write-in
// judges your typing and shows you the verdict, then you pick Again/Hard/Good/
// Easy exactly as before. A grader that fed FSRS directly would let a string
// comparison decide your schedule.

import { useEffect, useRef, useState } from "react";
import { Eye, Keyboard, Lightbulb, RotateCw } from "lucide-react";

import { hintFor, judge, missingWords, type Verdict } from "../lib/answerMatch";
import { Button, cx } from "./ui";
import { Kbd } from "./primitives";

export type StudyMode = "flip" | "write" | "hint";

export const MODES: {
  value: StudyMode;
  label: string;
  icon: typeof Eye;
  blurb: string;
}[] = [
  {
    value: "flip",
    label: "Flip",
    icon: RotateCw,
    blurb: "See it and judge yourself",
  },
  {
    value: "write",
    label: "Write",
    icon: Keyboard,
    blurb: "Type the answer out",
  },
  {
    value: "hint",
    label: "Hint",
    icon: Lightbulb,
    blurb: "Reveal it a piece at a time",
  },
];

export function ModePicker({
  mode,
  onChange,
}: {
  mode: StudyMode;
  onChange: (m: StudyMode) => void;
}) {
  return (
    <div className="flex gap-1">
      {MODES.map((m) => (
        <button
          key={m.value}
          onClick={() => onChange(m.value)}
          title={m.blurb}
          aria-pressed={mode === m.value}
          className={cx(
            "pressable flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12px]",
            mode === m.value
              ? "border-[var(--accent)]/40 bg-[var(--accent)]/12 text-[var(--accent)]"
              : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)] hover:text-[var(--ink)]",
          )}
        >
          <m.icon size={12} />
          {m.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Write-in mode.
 *
 * The verdict is advisory and says so. "Close" is the interesting case and the
 * reason the grader is three-valued rather than a boolean: it shows what you
 * left out, which teaches something, and then leaves the rating to you.
 */
export function WriteAnswer({
  expected,
  onSubmitted,
}: {
  expected: string;
  /** Fired once, so the parent can reveal the real answer beneath. */
  onSubmitted: () => void;
}) {
  const [typed, setTyped] = useState("");
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    ref.current?.focus();
  }, []);

  const submit = () => {
    if (verdict || !typed.trim()) return;
    setVerdict(judge(typed, expected));
    onSubmitted();
  };

  const missing = verdict === "close" ? missingWords(typed, expected) : [];

  return (
    <div>
      <textarea
        ref={ref}
        value={typed}
        onChange={(e) => setTyped(e.target.value)}
        onKeyDown={(e) => {
          // ⌘↵ submits so a multi-line answer can hold newlines.
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            submit();
          }
        }}
        readOnly={!!verdict}
        rows={3}
        placeholder="Write it out — recall is the point."
        className={cx(
          "w-full resize-none rounded-[var(--r-md)] border bg-[var(--surface-hi)] p-3.5",
          "text-[14.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)]",
          "outline-none transition-colors duration-[var(--t-fast)]",
          verdict === "correct"
            ? "border-[var(--color-positive)]/50"
            : verdict === "close"
              ? "border-[var(--warn)]/50"
              : verdict === "different"
                ? "border-[var(--danger)]/45"
                : "border-[var(--line)] focus:border-[var(--accent)]",
        )}
      />

      {!verdict ? (
        <div className="mt-2.5 flex items-center gap-2">
          <Button size="sm" disabled={!typed.trim()} onClick={submit}>
            Check
          </Button>
          <span className="flex items-center gap-1.5 text-[11.5px] text-[var(--ink-faint)]">
            <Kbd>⌘</Kbd>
            <Kbd>↵</Kbd>
          </span>
        </div>
      ) : (
        <div className="animate-rise mt-3">
          <div
            className="text-[13.5px] font-medium"
            style={{
              color:
                verdict === "correct"
                  ? "var(--color-positive)"
                  : verdict === "close"
                    ? "var(--warn)"
                    : "var(--danger)",
            }}
          >
            {verdict === "correct"
              ? "That matches."
              : verdict === "close"
                ? "Close."
                : "Not quite."}
          </div>

          {missing.length > 0 && (
            <p className="mt-1 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
              You didn't mention: {missing.join(", ")}.
            </p>
          )}

          {/* The judgement is a string comparison and shouldn't pretend
              otherwise — the rating below is still entirely yours. */}
          <p className="mt-1.5 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
            Read the real answer below and rate it yourself — this check is a
            rough guide, not a mark.
          </p>
        </div>
      )}
    </div>
  );
}

/**
 * Hint mode.
 *
 * Each press costs you something: the shape, then the first word, then half.
 * A hint button that jumps straight to the answer isn't a hint, it's a reveal
 * with extra steps.
 */
export function HintLadder({
  answer,
  onExhausted,
}: {
  answer: string;
  /** Fired when the last rung is used, so the parent can reveal in full. */
  onExhausted: () => void;
}) {
  const [level, setLevel] = useState(0);

  const labels = ["Show me the shape", "Open the first word", "Open half"];

  return (
    <div>
      {level > 0 && (
        <p className="animate-rise mb-3 font-mono text-[15px] leading-relaxed tracking-[0.04em] text-[var(--ink-dim)]">
          {hintFor(answer, level)}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {level < 3 && (
          <Button
            size="sm"
            onClick={() => {
              const next = level + 1;
              setLevel(next);
              if (next >= 3) onExhausted();
            }}
          >
            <Lightbulb size={13} />
            {labels[level]}
          </Button>
        )}
        <Button size="sm" variant="ghost" onClick={onExhausted}>
          <Eye size={13} />
          Just show me
        </Button>
      </div>
    </div>
  );
}

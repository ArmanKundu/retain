// Biology Units 3 & 4.
//
// A note on what this screen deliberately does not contain: Retain ships no
// VCAA study-design content. There are no dot points baked into the app,
// because the study design is VCAA's document, it changes between accreditation
// periods, and content invented to look plausible is worse than none — you'd
// revise against it and never find out. The topic tree below starts empty and
// is filled by pasting your own copy of the outline.

import { useCallback, useEffect, useState } from "react";
import {
  BookOpen,
  ChevronRight,
  Clock,
  FileText,
  Pause,
  Play,
  Square,
} from "lucide-react";

import { api } from "../lib/api";
import { isBiologyThreeFour } from "../lib/catalogue";
import { clock } from "../lib/format";
import type {
  CommandWord,
  DeckSummary,
  ExamState,
  OutlineRow,
  PracticeExam,
  Subject,
  TopicNode,
} from "../lib/types";
import { Button, Card, Empty, SectionTitle, cx } from "../components/ui";
import { useApp } from "../store";

/** Confidence 1–5 → a colour. Grey means never rated, not "bad". */
function confidenceTint(c: number | null): string {
  if (c == null) return "var(--line)";
  const hue = [0, 14, 32, 60, 110, 140][c] ?? 60;
  return `hsl(${hue} 55% 50%)`;
}

export function Biology() {
  const subjects = useApp((s) => s.subjects);
  const setRoute = useApp((s) => s.setRoute);

  // The Biology 3/4 subject, if there is one. Everything here is scoped to it.
  const subject = subjects.find(isBiologyThreeFour);

  const [exam, setExam] = useState<ExamState | null>(null);

  // The exam clock is derived from a stored start instant, so a one-second
  // poll is enough and nothing is lost if the window is closed.
  const refreshExam = useCallback(async () => {
    try {
      setExam(await api.examState());
    } catch {
      setExam(null);
    }
  }, []);

  useEffect(() => {
    void refreshExam();
    const t = setInterval(() => void refreshExam(), 1000);
    return () => clearInterval(t);
  }, [refreshExam]);

  if (!subject) {
    return (
      <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 sm:px-9 pb-14">
        <div className="titlebar-drag h-11" />
        <header className="mb-6">
          <h1 className="text-[24px] font-semibold tracking-[-0.025em]">Biology 3/4</h1>
        </header>
        <Card>
          <Empty
            title="No Biology 3/4 subject yet"
            body="Add a subject called Biology and set it to Units 3 & 4 in Settings. This screen holds the topic tree, exam simulation and command-word reference."
          />
          <div className="px-6 pb-6">
            <Button size="sm" onClick={() => setRoute("settings")}>
              Open Settings
            </Button>
          </div>
        </Card>
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 sm:px-9 pb-14">
      <div className="titlebar-drag h-11" />

      <header className="mb-6">
        <h1 className="text-[24px] font-semibold tracking-[-0.025em]">Biology 3/4</h1>
        <p className="mt-1 text-[13.5px] text-[var(--ink-dim)]">
          The topic tree, exam simulation, and what the command words are actually asking for.
        </p>
      </header>

      <ExamPanel subject={subject} exam={exam} onChange={refreshExam} />
      <TopicTree subject={subject} />
      <TerminologyPanel subject={subject} onAddCards={() => setRoute("import")} />
      <CommandWords />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Exam simulation
// ---------------------------------------------------------------------------

/**
 * 15 minutes reading, 2 hours 30 writing.
 *
 * Pausing is allowed but banked separately and shown on the result, so a
 * 40-minute attempt with three breaks doesn't get logged as a clean sitting.
 * The honest number is the one worth keeping.
 */
function ExamPanel({
  subject,
  exam,
  onChange,
}: {
  subject: Subject;
  exam: ExamState | null;
  onChange: () => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [history, setHistory] = useState<PracticeExam[]>([]);
  const [scoring, setScoring] = useState<PracticeExam | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadHistory = useCallback(async () => {
    try {
      setHistory(await api.examHistory(subject.id, 8));
    } catch {
      setHistory([]);
    }
  }, [subject.id]);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory, exam === null]);

  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await onChange();
      await loadHistory();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className="mb-7">
      <SectionTitle>Exam simulation</SectionTitle>

      <Card className="mt-2.5 p-5">
        {exam ? (
          <div>
            <div className="flex items-baseline gap-3">
              <span
                className={cx(
                  "rounded-full px-2.5 py-1 text-[11px] font-medium uppercase tracking-[0.06em]",
                  exam.phase === "reading"
                    ? "bg-[var(--accent)]/15 text-[var(--accent)]"
                    : exam.phase === "writing"
                      ? "bg-[color-mix(in_srgb,var(--warn)_15%,transparent)] text-[var(--warn)]"
                      : "bg-[var(--color-positive)]/15 text-[var(--color-positive)]",
                )}
              >
                {exam.phase === "reading"
                  ? "Reading time"
                  : exam.phase === "writing"
                    ? "Writing time"
                    : "Time is up"}
              </span>
              <span className="truncate text-[13px] text-[var(--ink-dim)]">{exam.run.name}</span>
            </div>

            <div className="tabular mt-3 text-[46px] font-medium leading-none tracking-[-0.03em]">
              {clock(exam.remainingSeconds)}
            </div>
            <div className="mt-1.5 text-[12.5px] text-[var(--ink-dim)]">
              {exam.phase === "finished"
                ? "Finish up and log the attempt."
                : `${clock(exam.elapsedSeconds)} elapsed${exam.paused ? " · paused" : ""}`}
            </div>

            {/* Progress across the whole paper, with the reading/writing
                boundary marked so the phase change isn't a surprise. */}
            <div className="relative mt-4 h-[6px] overflow-hidden rounded-full bg-[var(--surface-hi)]">
              <div
                className="h-full rounded-full bg-[var(--accent)] transition-[width] duration-1000 ease-linear"
                style={{
                  width: `${Math.min(100, (exam.elapsedSeconds / exam.totalSeconds) * 100)}%`,
                }}
              />
              <div
                className="absolute top-0 h-full w-px bg-[var(--canvas)]"
                style={{ left: `${(900 / exam.totalSeconds) * 100}%` }}
              />
            </div>

            <div className="mt-4 flex flex-wrap gap-2">
              {exam.phase !== "finished" && (
                <Button size="sm" onClick={() => void act(() => api.setExamPaused(!exam.paused))}>
                  {exam.paused ? <Play size={13} /> : <Pause size={13} />}
                  {exam.paused ? "Resume" : "Pause"}
                </Button>
              )}
              <Button size="sm" variant="primary" onClick={() => void act(api.finishExam)}>
                <Square size={12} />
                Finish and log
              </Button>
              <Button size="sm" variant="ghost" onClick={() => void act(api.cancelExam)}>
                Discard
              </Button>
            </div>

            {exam.paused && (
              <p className="mt-3 text-[12px] leading-relaxed text-[var(--ink-faint)]">
                Paused time is tracked separately and won't be counted as exam time.
              </p>
            )}
          </div>
        ) : (
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Which paper? e.g. 2023 VCAA"
                className="h-9 min-w-[200px] flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
              />
              <Button
                variant="primary"
                onClick={() => void act(() => api.startExam(subject.id, name)).then(() => setName(""))}
              >
                <Clock size={14} />
                Start
              </Button>
            </div>
            <p className="mt-3 text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
              15 minutes reading, then 2 hours 30 writing. The clock keeps running if you close
              the window — it's stored as a start time, not a countdown, so quitting and
              reopening picks up where you actually are.
            </p>
          </div>
        )}

        {error && <p className="mt-3 text-[12.5px] text-[var(--danger)]">{error}</p>}
      </Card>

      {history.length > 0 && (
        <Card className="mt-2.5 divide-y divide-[var(--line-soft)] overflow-hidden">
          {history.map((h) => (
            <div key={h.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13.5px]">{h.name}</div>
                <div className="text-[11.5px] text-[var(--ink-faint)]">
                  {h.takenOn}
                  {h.writingSeconds != null && ` · ${Math.round(h.writingSeconds / 60)} min writing`}
                </div>
              </div>

              {h.sectionAScore != null || h.sectionBScore != null ? (
                <div className="tabular text-[12.5px] text-[var(--ink-dim)]">
                  A {h.sectionAScore ?? "–"}/{h.sectionAMax} · B {h.sectionBScore ?? "–"}/
                  {h.sectionBMax}
                </div>
              ) : (
                <Button size="sm" variant="ghost" onClick={() => setScoring(h)}>
                  Add score
                </Button>
              )}
            </div>
          ))}
        </Card>
      )}

      {scoring && (
        <ScoreDialog
          exam={scoring}
          onClose={() => setScoring(null)}
          onSaved={async () => {
            setScoring(null);
            await loadHistory();
          }}
        />
      )}
    </section>
  );
}

function ScoreDialog({
  exam,
  onClose,
  onSaved,
}: {
  exam: PracticeExam;
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const [a, setA] = useState("");
  const [b, setB] = useState("");

  const num = (v: string) => (v.trim() === "" ? null : Number(v));

  return (
    <div className="scrim fixed inset-0 z-50 flex items-center justify-center px-8">
      <div className="sheet animate-pop w-full max-w-[400px] p-6">
        <SectionTitle>{exam.name}</SectionTitle>

        <div className="mt-3 space-y-3">
          {[
            { label: `Section A (out of ${exam.sectionAMax})`, value: a, set: setA },
            { label: `Section B (out of ${exam.sectionBMax})`, value: b, set: setB },
          ].map((f) => (
            <div key={f.label}>
              <label className="text-[12.5px] text-[var(--ink-dim)]">{f.label}</label>
              <input
                type="number"
                value={f.value}
                onChange={(e) => f.set(e.target.value)}
                className="tabular mt-1 h-9 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[13px] text-[var(--ink)] outline-none focus:border-[var(--accent)]"
              />
            </div>
          ))}
        </div>

        <div className="mt-4 flex gap-2">
          <Button
            variant="primary"
            size="sm"
            onClick={async () => {
              await api.scoreExam(exam.id, num(a), num(b));
              await onSaved();
            }}
          >
            Save
          </Button>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Topic tree
// ---------------------------------------------------------------------------

function TopicTree({ subject }: { subject: Subject }) {
  const [tree, setTree] = useState<TopicNode[]>([]);
  const [importing, setImporting] = useState(false);

  const load = useCallback(async () => {
    try {
      setTree(await api.topicTree(subject.id));
    } catch {
      setTree([]);
    }
  }, [subject.id]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <section className="mb-7">
      <div className="flex items-center gap-3">
        <SectionTitle>Topics</SectionTitle>
        <button
          onClick={() => setImporting(!importing)}
          className="ml-auto text-[12px] text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
        >
          {importing ? "Close" : tree.length ? "Replace outline" : "Paste your outline"}
        </button>
      </div>

      {importing && (
        <OutlineImporter
          subject={subject}
          hasExisting={tree.length > 0}
          onDone={async () => {
            setImporting(false);
            await load();
          }}
        />
      )}

      <Card className="mt-2.5">
        {tree.length === 0 ? (
          <Empty
            title="No topics yet"
            body="Paste your Unit 3 & 4 outline above and Retain builds the tree from it. Nothing about the course is built into the app — the content comes from your copy of the study design."
          />
        ) : (
          <div className="py-1">
            {tree.map((n) => (
              <TopicRow key={n.id} node={n} depth={0} />
            ))}
          </div>
        )}
      </Card>
    </section>
  );
}

function TopicRow({ node, depth }: { node: TopicNode; depth: number }) {
  // Units and areas of study start open; dot points have no children anyway.
  const [open, setOpen] = useState(depth < 1);
  const hasChildren = node.children.length > 0;

  return (
    <>
      <div
        className="group flex items-center gap-2 px-4 py-[7px] transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
        style={{ paddingLeft: `${16 + depth * 18}px` }}
      >
        {hasChildren ? (
          <button
            onClick={() => setOpen(!open)}
            aria-expanded={open}
            aria-label={open ? `Collapse ${node.name}` : `Expand ${node.name}`}
            className="shrink-0 text-[var(--ink-faint)] transition-transform duration-150 hover:text-[var(--ink)]"
            style={{ transform: open ? "rotate(90deg)" : undefined }}
          >
            <ChevronRight size={13} />
          </button>
        ) : (
          <span
            className="ml-[2px] h-[7px] w-[7px] shrink-0 rounded-full"
            style={{ background: confidenceTint(node.confidence) }}
            title={node.confidence ? `Confidence ${node.confidence}/5` : "Never rated"}
          />
        )}

        <span
          className={cx(
            "min-w-0 flex-1 truncate",
            depth === 0 ? "text-[13.5px] font-medium" : "text-[13px] text-[var(--ink-dim)]",
          )}
        >
          {node.name}
        </span>

        <div className="flex shrink-0 items-center gap-2.5 text-[11px] text-[var(--ink-faint)]">
          {node.cardCount > 0 && <span>{node.cardCount} cards</span>}
          {node.errorCount > 0 && <span className="text-[var(--warn)]">{node.errorCount} errors</span>}
          {node.lastReviewedOn && <span>{node.lastReviewedOn.slice(5)}</span>}
        </div>
      </div>

      {open && node.children.map((c) => <TopicRow key={c.id} node={c} depth={depth + 1} />)}
    </>
  );
}

/**
 * Paste an outline, see exactly what it will become, then commit.
 *
 * The preview is the point. Importing replaces the existing topics, so showing
 * the parsed shape first is what makes a destructive action safe to take.
 */
function OutlineImporter({
  subject,
  hasExisting,
  onDone,
}: {
  subject: Subject;
  hasExisting: boolean;
  onDone: () => Promise<void>;
}) {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<OutlineRow[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!text.trim()) {
      setPreview([]);
      return;
    }
    const t = setTimeout(() => {
      void api.previewTopicOutline(text).then(setPreview).catch(() => setPreview([]));
    }, 150);
    return () => clearTimeout(t);
  }, [text]);

  return (
    <Card className="animate-in mt-2.5 p-5">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
        placeholder={"Unit 3\n  Area of Study 1\n    first dot point\n    second dot point\n  Area of Study 2\nUnit 4"}
        className="selectable h-40 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 font-mono text-[12.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
      />

      <p className="mt-2.5 text-[12px] leading-relaxed text-[var(--ink-faint)]">
        Indent with spaces or tabs to nest. Bullets and numbering are stripped. Copy this straight
        out of your own study design — Retain doesn't ship VCAA content.
      </p>

      {preview.length > 0 && (
        <div className="mt-3 max-h-44 overflow-y-auto rounded-[var(--r-sm)] border border-[var(--line-soft)] py-1.5">
          {preview.map((r, i) => (
            <div
              key={i}
              className="truncate py-[3px] text-[12px] text-[var(--ink-dim)]"
              style={{ paddingLeft: `${12 + r.depth * 16}px` }}
            >
              {r.name}
            </div>
          ))}
        </div>
      )}

      {hasExisting && preview.length > 0 && (
        <p className="mt-3 text-[12.5px] leading-relaxed text-[var(--warn)]">
          This replaces the current topics. Cards and error entries stay, but lose their topic
          link.
        </p>
      )}

      <div className="mt-3 flex items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          disabled={preview.length === 0}
          onClick={async () => {
            setError(null);
            try {
              await api.importTopicOutline(subject.id, text);
              await onDone();
            } catch (e) {
              setError(String(e));
            }
          }}
        >
          Import {preview.length > 0 && `${preview.length} topics`}
        </Button>
        {error && <span className="text-[12.5px] text-[var(--danger)]">{error}</span>}
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Terminology
// ---------------------------------------------------------------------------

function TerminologyPanel({
  subject,
  onAddCards,
}: {
  subject: Subject;
  onAddCards: () => void;
}) {
  const [deck, setDeck] = useState<DeckSummary | null>(null);

  useEffect(() => {
    void api
      .terminologySummary(subject.id)
      .then(setDeck)
      .catch(() => setDeck(null));
  }, [subject.id]);

  if (!deck) return null;

  return (
    <section className="mb-7">
      <SectionTitle>Terminology deck</SectionTitle>
      <Card className="mt-2.5 p-5">
        {deck.total === 0 ? (
          <>
            <p className="text-[13px] leading-relaxed text-[var(--ink-dim)]">
              Add Biology cards with the tag{" "}
              <code className="rounded bg-[var(--surface-hi)] px-1.5 py-0.5 font-mono text-[12px]">
                terminology
              </code>{" "}
              and they'll be tracked here as their own deck. Precise wording is most of the
              difference between a 1-mark and a 2-mark answer.
            </p>
            <Button size="sm" className="mt-3" onClick={onAddCards}>
              <BookOpen size={13} />
              Add cards
            </Button>
          </>
        ) : (
          <div className="flex flex-wrap items-center gap-x-8 gap-y-3">
            <Figure value={deck.total} label="terms" />
            <Figure value={deck.due} label="due now" />
            <Figure value={deck.new} label="not started" />
            <Button size="sm" variant="ghost" className="ml-auto" onClick={onAddCards}>
              Add more
            </Button>
          </div>
        )}
      </Card>
    </section>
  );
}

function Figure({ value, label }: { value: number; label: string }) {
  return (
    <div>
      <div className="tabular text-[21px] font-medium tracking-[-0.02em]">{value}</div>
      <div className="mt-0.5 text-[11.5px] text-[var(--ink-faint)]">{label}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Command words
// ---------------------------------------------------------------------------

function CommandWords() {
  const [words, setWords] = useState<CommandWord[]>([]);

  useEffect(() => {
    void api.commandWords().then(setWords).catch(() => setWords([]));
  }, []);

  if (words.length === 0) return null;

  return (
    <section>
      <SectionTitle>Command words</SectionTitle>
      <Card className="mt-2.5 divide-y divide-[var(--line-soft)] overflow-hidden">
        {words.map((w) => (
          <div key={w.word} className="flex gap-4 px-5 py-2.5">
            <span className="w-[92px] shrink-0 text-[13px] font-medium">{w.word}</span>
            <span className="flex-1 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
              {w.meaning}
            </span>
          </div>
        ))}

        <p className="flex items-start gap-2 px-5 py-3.5 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
          <FileText size={12} className="mt-[2px] shrink-0" />
          Plain-English guidance written for this app, not a copy of VCAA's glossary. Check the
          study design for the official wording.
        </p>
      </Card>
    </section>
  );
}

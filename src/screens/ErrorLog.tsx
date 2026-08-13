import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, Eye, Lock, Plus, Search, Trash2, X } from "lucide-react";

import { AiAction, useAi } from "../components/Ai";
import { Button, Card, ColourDot, Empty, SectionTitle, cx } from "../components/ui";
import { api } from "../lib/api";
import { prettyDate } from "../lib/format";
import type {
  BlindPrompt,
  CategoryCount,
  CommandWord,
  EntryFilter,
  ErrorEntry,
  SelfAssessment,
  Subject,
} from "../lib/types";
import { useApp } from "../store";

/**
 * The error log.
 *
 * The part that matters is `ReattemptModal`. Everything about it is arranged so
 * the mark scheme cannot be read before you've written your own answer:
 *
 *   * the prompt object the backend sends has no answer field in it at all;
 *   * the reveal button doesn't exist until the answer is committed;
 *   * committing is one-way — you cannot edit after seeing the scheme; and
 *   * "fixed" is only reachable by self-marking a committed attempt correct.
 *
 * If any of that were only a UI convention, the feature would quietly become
 * "re-read the answer", which is the failure mode the whole log exists to avoid.
 */

/** Stable colour per category so the list is scannable at a glance. */
function categoryTint(category: string): string {
  let h = 0;
  for (const ch of category) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return `hsl(${h} 55% 55%)`;
}

export function ErrorLog() {
  const subjects = useApp((s) => s.subjects);

  const [entries, setEntries] = useState<ErrorEntry[]>([]);
  const [recurring, setRecurring] = useState<CategoryCount[]>([]);
  const [due, setDue] = useState<number[]>([]);
  const [filter, setFilter] = useState<EntryFilter>({ onlyUnfixed: false });
  const [search, setSearch] = useState("");
  const [composing, setComposing] = useState(false);
  const [prompt, setPrompt] = useState<BlindPrompt | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const active: EntryFilter = { ...filter, search: search.trim() || null };
      const [list, top, dueIds] = await Promise.all([
        api.listErrorEntries(active),
        api.recurringErrors(filter.subjectId ?? null, 30),
        api.dueErrorReattempts(filter.subjectId ?? null),
      ]);
      setEntries(list);
      setRecurring(top);
      setDue(dueIds);
    } catch (e) {
      setError(String(e));
    }
  }, [filter, search]);

  useEffect(() => {
    const t = setTimeout(() => void load(), 160);
    return () => clearTimeout(t);
  }, [load]);

  const startReattempt = async (entryId: number) => {
    try {
      setPrompt(await api.startErrorReattempt(entryId));
    } catch (e) {
      setError(String(e));
    }
  };

  const categories = useMemo(
    () => Array.from(new Set(entries.map((e) => e.category))).sort(),
    [entries],
  );

  return (
    <div className="mx-auto w-full max-w-[min(980px,100%)] px-6 sm:px-9 pb-14">
      <div className="titlebar-drag h-11" />

      <header className="mb-6 flex items-center">
        <div>
          <h1 className="text-[24px] font-semibold tracking-[-0.025em]">Error log</h1>
          <p className="mt-1 text-[13.5px] text-[var(--ink-dim)]">
            Every mistake worth learning from, and a blind re-attempt a week later.
          </p>
        </div>
        <Button variant="primary" className="ml-auto" onClick={() => setComposing(true)}>
          <Plus size={15} />
          Log an error
        </Button>
      </header>

      {error && (
        <div className="mb-4 flex items-start gap-2 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] px-4 py-3 text-[13px] text-[var(--danger)]">
          <span className="flex-1">{error}</span>
          <button onClick={() => setError(null)}>
            <X size={14} />
          </button>
        </div>
      )}

      {/* Due blind re-attempts */}
      {due.length > 0 && (
        <Card className="animate-in mb-4 border-[var(--accent)]/30 bg-[var(--accent)]/8 p-4">
          <div className="flex items-center gap-3">
            <Lock size={16} className="text-[var(--accent)]" />
            <div className="flex-1 text-[13.5px] leading-relaxed">
              <span className="font-medium">
                {due.length} {due.length === 1 ? "error is" : "errors are"} ready for a blind
                re-attempt
              </span>
              <div className="text-[12.5px] text-[var(--ink-dim)]">
                You'll answer from scratch. The mark scheme stays hidden until you've committed.
              </div>
            </div>
            <Button variant="primary" size="sm" onClick={() => void startReattempt(due[0])}>
              Start
            </Button>
          </div>
        </Card>
      )}

      {/* Recurring analytics */}
      {recurring.length > 0 && (
        <section className="mb-5">
          <SectionTitle>What keeps happening — last 30 days</SectionTitle>
          <div className="mt-2.5 grid grid-cols-3 gap-3">
            {recurring.slice(0, 3).map((c, i) => (
              <Card key={c.category} className="p-4">
                <div className="flex items-baseline gap-2">
                  <span className="tabular text-[22px] font-semibold leading-none">{c.count}</span>
                  <span className="text-[11px] text-[var(--ink-faint)]">#{i + 1}</span>
                </div>
                <div className="mt-1.5 text-[12.5px] leading-snug text-[var(--ink)]">
                  {c.category}
                </div>
                {c.marksLost > 0 && (
                  <div className="mt-0.5 text-[11.5px] text-[var(--ink-faint)]">
                    {c.marksLost} marks lost
                  </div>
                )}
              </Card>
            ))}
          </div>
        </section>
      )}

      {/* Filters */}
      <Card className="mb-4 p-4">
        <div className="flex flex-wrap items-center gap-1.5">
          <button
            onClick={() => setFilter({ ...filter, subjectId: null })}
            className={cx(
              "rounded-full border px-2.5 py-1 text-[12px] transition-colors",
              filter.subjectId == null
                ? "border-[var(--ink-faint)] text-[var(--ink)]"
                : "border-[var(--line)] text-[var(--ink-faint)] hover:border-[var(--ink-faint)]",
            )}
          >
            All subjects
          </button>
          {subjects.map((s) => (
            <button
              key={s.id}
              onClick={() => setFilter({ ...filter, subjectId: s.id })}
              className={cx(
                "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12px] transition-colors",
                filter.subjectId === s.id
                  ? "border-[var(--ink-faint)] text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-faint)] hover:border-[var(--ink-faint)]",
              )}
            >
              <ColourDot colour={s.colour} size={7} />
              {s.name}
            </button>
          ))}
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <div className="relative flex-1">
            <Search
              size={13}
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--ink-faint)]"
            />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search questions, answers, takeaways…"
              className="h-8 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] pl-8 pr-2.5 text-[13px] text-[var(--ink)]"
            />
          </div>

          <select
            value={filter.category ?? ""}
            onChange={(e) => setFilter({ ...filter, category: e.target.value || null })}
            className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
          >
            <option value="">All categories</option>
            {categories.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>

          <button
            onClick={() => setFilter({ ...filter, onlyUnfixed: !filter.onlyUnfixed })}
            className={cx(
              "h-8 rounded-[var(--r-sm)] border px-2.5 text-[12.5px] transition-colors",
              filter.onlyUnfixed
                ? "border-[var(--ink-faint)] bg-[var(--surface-hi)] text-[var(--ink)]"
                : "border-[var(--line)] text-[var(--ink-faint)]",
            )}
          >
            Unfixed only
          </button>
        </div>
      </Card>

      {/* Entries */}
      {entries.length === 0 ? (
        <Card>
          <Empty
            title="Nothing logged yet"
            body="Log a question you got wrong — what you wrote, what the mark scheme wanted, and the one-sentence takeaway. In a week you'll be asked it again, blind."
          />
        </Card>
      ) : (
        <div className="space-y-2.5">
          {entries.map((e) => (
            <EntryCard
              key={e.id}
              entry={e}
              onReattempt={() => void startReattempt(e.id)}
              onDelete={async () => {
                await api.deleteErrorEntry(e.id);
                await load();
              }}
            />
          ))}
        </div>
      )}

      {composing && (
        <ComposeModal
          subjects={subjects}
          onClose={() => setComposing(false)}
          onSaved={async () => {
            setComposing(false);
            await load();
          }}
        />
      )}

      {prompt && (
        <ReattemptModal
          prompt={prompt}
          onClose={async () => {
            setPrompt(null);
            await load();
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------

function EntryCard({
  entry,
  onReattempt,
  onDelete,
}: {
  entry: ErrorEntry;
  onReattempt: () => void;
  onDelete: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);

  return (
    <Card className="overflow-hidden">
      <button className="w-full px-5 py-3.5 text-left" onClick={() => setOpen(!open)}>
        <div className="flex items-center gap-2.5">
          <span
            className="h-full w-1 shrink-0 self-stretch rounded-full"
            style={{ background: categoryTint(entry.category) }}
          />
          <ColourDot colour={entry.colour} size={8} />
          <span className="text-[12px] text-[var(--ink-faint)]">{entry.subjectName}</span>
          <span
            className="rounded-full px-2 py-0.5 text-[11px]"
            style={{
              background: `${categoryTint(entry.category)}22`,
              color: categoryTint(entry.category),
            }}
          >
            {entry.category}
          </span>
          {entry.fixedAt && (
            <span className="flex items-center gap-1 text-[11.5px] text-[var(--color-positive)]">
              <Check size={12} />
              fixed
            </span>
          )}
          <span className="tabular ml-auto text-[11.5px] text-[var(--ink-faint)]">
            {entry.marksLost != null && entry.marksAvailable != null
              ? `−${entry.marksLost}/${entry.marksAvailable}`
              : ""}
          </span>
        </div>

        <div className="selectable mt-2 line-clamp-2 text-[13.5px] leading-relaxed text-[var(--ink)]">
          {entry.questionText ?? entry.source ?? "(no question text)"}
        </div>

        {entry.fix && (
          <div className="selectable mt-1 text-[12.5px] italic text-[var(--ink-dim)]">
            {entry.fix}
          </div>
        )}
      </button>

      {open && (
        <div className="animate-in border-t border-[var(--line-soft)] px-5 py-4">
          <dl className="space-y-3 text-[13px]">
            {entry.source && <Field label="Source">{entry.source}</Field>}
            {entry.commandWord && <Field label="Command word">{entry.commandWord}</Field>}
            {entry.topicName && <Field label="Topic">{entry.topicName}</Field>}
            {entry.myAnswer && <Field label="What I wrote">{entry.myAnswer}</Field>}
            {/*
              The mark scheme is shown here deliberately: this is the review
              view for an entry you are editing, not a re-attempt. The blind
              path never renders this component.
            */}
            {entry.correctAnswer && <Field label="Mark scheme">{entry.correctAnswer}</Field>}
            <Field label="Logged">{prettyDate(entry.loggedOn)}</Field>
            {entry.revisitOn && !entry.fixedAt && (
              <Field label="Blind re-attempt due">{prettyDate(entry.revisitOn)}</Field>
            )}
            {entry.reattemptCount > 0 && (
              <Field label="Attempts">{entry.reattemptCount}</Field>
            )}
          </dl>

          <div className="mt-4 flex items-center gap-2">
            {!entry.fixedAt && (
              <Button size="sm" onClick={onReattempt}>
                <Lock size={13} />
                Re-attempt blind
              </Button>
            )}
            <button
              onClick={onDelete}
              className="ml-auto flex items-center gap-1.5 text-[12.5px] text-[var(--ink-faint)] transition-colors hover:text-[var(--danger)]"
            >
              <Trash2 size={13} />
              Delete
            </button>
          </div>
        </div>
      )}
    </Card>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <dt className="text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">{label}</dt>
      <dd className="selectable mt-0.5 leading-relaxed text-[var(--ink)]">{children}</dd>
    </div>
  );
}

// ---------------------------------------------------------------------------
// The blind re-attempt
// ---------------------------------------------------------------------------

function ReattemptModal({
  prompt,
  onClose,
}: {
  prompt: BlindPrompt;
  onClose: () => Promise<void>;
}) {
  const [answer, setAnswer] = useState("");
  const [committed, setCommitted] = useState(false);
  // Only ever populated by `revealErrorAnswer`, which the backend refuses to
  // serve until `committed` is true.
  const [scheme, setScheme] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [marks, setMarks] = useState("");
  const [outcome, setOutcome] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const commit = async () => {
    if (!answer.trim()) return;
    setBusy(true);
    try {
      await api.commitErrorReattempt(prompt.reattemptId, answer);
      setCommitted(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const reveal = async () => {
    setBusy(true);
    try {
      setScheme(await api.revealErrorAnswer(prompt.reattemptId));
      setRevealed(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const assess = async (a: SelfAssessment) => {
    setBusy(true);
    try {
      const m = marks.trim() === "" ? null : Number(marks);
      setOutcome(
        await api.assessErrorReattempt(prompt.reattemptId, a, Number.isNaN(m as number) ? null : m),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 px-8 backdrop-blur-[2px]">
      <div className="glass animate-pop max-h-[86vh] w-full max-w-[620px] overflow-y-auto rounded-[var(--r-xl)] p-6">
        <div className="flex items-center gap-2">
          <ColourDot colour={prompt.colour} size={8} />
          <span className="text-[12.5px] text-[var(--ink-faint)]">{prompt.subjectName}</span>
          {prompt.source && (
            <span className="text-[12.5px] text-[var(--ink-faint)]">· {prompt.source}</span>
          )}
          {prompt.marksAvailable != null && (
            <span className="ml-auto text-[12.5px] text-[var(--ink-faint)]">
              {prompt.marksAvailable} marks
            </span>
          )}
        </div>

        <p className="selectable mt-4 text-[17px] leading-relaxed">
          {prompt.questionText ?? "(no question text recorded)"}
        </p>

        {prompt.commandWord && (
          <p className="mt-2 text-[12.5px] text-[var(--ink-faint)]">
            Command word: <span className="text-[var(--ink-dim)]">{prompt.commandWord}</span>
          </p>
        )}

        {prompt.marksAvailable != null && prompt.marksAvailable > 1 && !committed && (
          <p className="mt-3 rounded-[var(--r-sm)] bg-[var(--surface-hi)] px-3 py-2 text-[12.5px] text-[var(--ink-dim)]">
            {prompt.marksAvailable} marks available — did you make at least one distinct point per
            mark?
          </p>
        )}

        {/* Step 1: write it out, blind */}
        <div className="mt-5">
          <div className="flex items-center gap-1.5 text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">
            {committed ? <Check size={12} /> : <Lock size={12} />}
            {committed ? "Your committed answer" : "Your answer — written from scratch"}
          </div>
          <textarea
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
            readOnly={committed}
            autoFocus
            placeholder="Answer it properly, as if it were the exam."
            className={cx(
              "selectable mt-2 h-36 w-full resize-none rounded-[var(--r-md)] border p-3 text-[14px] leading-relaxed",
              committed
                ? "border-[var(--line-soft)] bg-[var(--surface)] text-[var(--ink-dim)]"
                : "border-[var(--line)] bg-[var(--surface-hi)] text-[var(--ink)] focus:border-[var(--accent)]",
            )}
          />
        </div>

        {error && <div className="mt-3 text-[12.5px] text-[var(--danger)]">{error}</div>}

        {/* Step 2: only after committing does a reveal control exist at all */}
        {!committed ? (
          <Button
            size="lg"
            variant="primary"
            className="mt-4 w-full"
            disabled={!answer.trim() || busy}
            onClick={commit}
          >
            Commit answer
          </Button>
        ) : !revealed ? (
          <Button size="lg" className="mt-4 w-full" disabled={busy} onClick={reveal}>
            <Eye size={15} />
            Show the mark scheme
          </Button>
        ) : (
          <>
            <div className="mt-4 rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface-hi)] p-4">
              <div className="text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">
                Mark scheme
              </div>
              <p className="selectable mt-1.5 text-[14px] leading-relaxed">
                {scheme ?? "(no mark scheme was recorded for this entry)"}
              </p>
            </div>

            {outcome === null ? (
              <div className="mt-4">
                <div className="flex items-center gap-2">
                  <span className="text-[12.5px] text-[var(--ink-dim)]">Marks you'd award</span>
                  <input
                    type="number"
                    min={0}
                    max={prompt.marksAvailable ?? 20}
                    value={marks}
                    onChange={(e) => setMarks(e.target.value)}
                    className="tabular h-7 w-[58px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[12.5px]"
                  />
                  {prompt.marksAvailable != null && (
                    <span className="text-[12.5px] text-[var(--ink-faint)]">
                      / {prompt.marksAvailable}
                    </span>
                  )}
                </div>

                <div className="mt-3 grid grid-cols-3 gap-2">
                  <Button size="sm" disabled={busy} onClick={() => void assess("incorrect")}>
                    Missed it
                  </Button>
                  <Button size="sm" disabled={busy} onClick={() => void assess("partial")}>
                    Partly
                  </Button>
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={busy}
                    onClick={() => void assess("correct")}
                  >
                    Got it
                  </Button>
                </div>
                <p className="mt-2 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
                  Only "Got it" marks this fixed. Anything else schedules another blind attempt in a
                  week.
                </p>
              </div>
            ) : (
              <div className="mt-4 flex items-center gap-2 rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface-hi)] p-3 text-[13px]">
                {outcome ? (
                  <>
                    <Check size={15} className="text-[var(--color-positive)]" />
                    <span>Fixed — you answered it correctly without seeing the scheme.</span>
                  </>
                ) : (
                  <>
                    <AlertTriangle size={15} className="text-[var(--warn)]" />
                    <span>Logged. It'll come back for another blind attempt in a week.</span>
                  </>
                )}
              </div>
            )}
          </>
        )}

        <button
          onClick={() => void onClose()}
          className="mt-4 w-full text-[12.5px] text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
        >
          {outcome === null && !committed ? "Not now" : "Close"}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function ComposeModal({
  subjects,
  onClose,
  onSaved,
}: {
  subjects: Subject[];
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const [subjectId, setSubjectId] = useState(subjects[0]?.id ?? 0);
  const { enabled: aiEnabled } = useAi();
  // undefined = not asked yet; null = asked, nothing in the allowed list.
  const [suggested, setSuggested] = useState<string | null | undefined>(undefined);
  const [categories, setCategories] = useState<string[]>([]);
  const [words, setWords] = useState<CommandWord[]>([]);
  const [form, setForm] = useState({
    source: "",
    commandWord: "",
    questionText: "",
    myAnswer: "",
    correctAnswer: "",
    category: "",
    fix: "",
    marksLost: "",
    marksAvailable: "",
  });
  const [image, setImage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const subject = subjects.find((s) => s.id === subjectId);
  const isThreeFour = subject?.unitLevel === "3_4";

  useEffect(() => {
    if (!subject) return;
    void api.errorCategories(subject.id).then((c) => {
      setCategories(c);
      setForm((f) => ({ ...f, category: c[0] ?? "" }));
    });
    // Keyed on id AND level: changing a subject to 3/4 changes its categories.
  }, [subject?.id, subject?.subjectType, subject?.unitLevel]);

  useEffect(() => {
    void api.commandWords().then(setWords);
  }, []);

  /** Paste a screenshot straight in — the fastest way to log a diagram question. */
  const onPaste = (e: React.ClipboardEvent) => {
    const file = Array.from(e.clipboardData.files).find((f) => f.type.startsWith("image/"));
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => setImage(reader.result as string);
    reader.readAsDataURL(file);
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.createErrorEntry({
        subjectId,
        topicId: null,
        source: form.source || null,
        commandWord: form.commandWord || null,
        questionText: form.questionText || null,
        questionImage: image,
        myAnswer: form.myAnswer || null,
        correctAnswer: form.correctAnswer || null,
        category: form.category,
        fix: form.fix || null,
        marksLost: form.marksLost ? Number(form.marksLost) : null,
        marksAvailable: form.marksAvailable ? Number(form.marksAvailable) : null,
      });
      await onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const set = (k: keyof typeof form) => (v: string) => setForm({ ...form, [k]: v });
  const activeWord = words.find((w) => w.word === form.commandWord);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 px-8 backdrop-blur-[2px]">
      <div
        onPaste={onPaste}
        className="glass animate-pop max-h-[88vh] w-full max-w-[620px] overflow-y-auto rounded-[var(--r-xl)] p-6"
      >
        <h2 className="text-[17px] font-semibold tracking-[-0.01em]">Log an error</h2>

        <div className="mt-4 flex flex-wrap gap-1.5">
          {subjects.map((s) => (
            <button
              key={s.id}
              onClick={() => setSubjectId(s.id)}
              className={cx(
                "flex items-center gap-2 rounded-full border px-3 py-1.5 text-[13px] transition-all active:scale-[0.97]",
                subjectId === s.id
                  ? "border-[var(--ink-faint)] bg-[var(--surface-hi)] text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-dim)]",
              )}
            >
              <ColourDot colour={s.colour} size={8} />
              {s.name}
            </button>
          ))}
        </div>

        <div className="mt-4 space-y-3">
          <Row label="Source">
            <input
              value={form.source}
              onChange={(e) => set("source")(e.target.value)}
              placeholder="2024 VCAA exam, Q7b"
              className={INPUT}
            />
          </Row>

          <Row label="The question">
            <textarea
              value={form.questionText}
              onChange={(e) => set("questionText")(e.target.value)}
              placeholder="Paste the question — or paste a screenshot anywhere in this dialog."
              className={cx(INPUT, "h-20 resize-none py-2")}
            />
          </Row>

          {image && (
            <div className="relative">
              <img src={image} alt="Pasted question" className="max-h-52 rounded-[var(--r-sm)] border border-[var(--line-soft)]" />
              <button
                onClick={() => setImage(null)}
                className="absolute right-2 top-2 rounded-full bg-black/60 p-1 text-white"
              >
                <X size={12} />
              </button>
            </div>
          )}

          <Row label="What I wrote">
            <textarea
              value={form.myAnswer}
              onChange={(e) => set("myAnswer")(e.target.value)}
              className={cx(INPUT, "h-16 resize-none py-2")}
            />
          </Row>

          <Row label="Mark scheme / correct answer">
            <textarea
              value={form.correctAnswer}
              onChange={(e) => set("correctAnswer")(e.target.value)}
              className={cx(INPUT, "h-16 resize-none py-2")}
            />
          </Row>

          <Row label="Category">
            <select
              value={form.category}
              onChange={(e) => set("category")(e.target.value)}
              className={INPUT}
            >
              {categories.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
          </Row>

          {/* Suggestion only. It moves the dropdown above; it never submits, and
              a suggestion outside the allowed list is dropped rather than
              guessed at — a wrong category quietly logged would corrupt the
              recurring-mistake analysis, which is the point of this screen. */}
          {aiEnabled && (
            <Row label="">
              <div className="flex items-center gap-3">
                <AiAction
                  label="Suggest a category"
                  disabled={!form.myAnswer.trim() || !form.correctAnswer.trim()}
                  run={() =>
                    api.aiSuggestCategory(
                      subjectId,
                      form.questionText,
                      form.myAnswer,
                      form.correctAnswer,
                    )
                  }
                  onDone={(c) => {
                    setSuggested(c);
                    if (c) set("category")(c);
                  }}
                />
                {suggested === null && (
                  <span className="text-[12px] text-[var(--ink-faint)]">
                    Didn't match one of the categories — pick it yourself.
                  </span>
                )}
                {suggested && (
                  <span className="text-[12px] text-[var(--ink-faint)]">
                    Suggested — change it if it's wrong.
                  </span>
                )}
              </div>
            </Row>
          )}

          {/* Command words are a 3/4 concern — the brief scopes them that way. */}
          {isThreeFour && (
            <Row label="Command word">
              <select
                value={form.commandWord}
                onChange={(e) => set("commandWord")(e.target.value)}
                className={INPUT}
              >
                <option value="">—</option>
                {words.map(({ word: w }) => (
                  <option key={w} value={w}>
                    {w}
                  </option>
                ))}
              </select>
              {activeWord && (
                <p className="mt-1.5 text-[12px] leading-relaxed text-[var(--ink-faint)]">
                  {activeWord.meaning}
                </p>
              )}
            </Row>
          )}

          <Row label="Fix — one sentence">
            <input
              value={form.fix}
              onChange={(e) => set("fix")(e.target.value)}
              placeholder="Name the bond type, don't just say 'it breaks'."
              className={INPUT}
            />
          </Row>

          <div className="flex items-center gap-2">
            <span className="text-[12.5px] text-[var(--ink-dim)]">Marks</span>
            <input
              type="number"
              value={form.marksLost}
              onChange={(e) => set("marksLost")(e.target.value)}
              placeholder="lost"
              className="tabular h-8 w-[70px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[12.5px]"
            />
            <span className="text-[12.5px] text-[var(--ink-faint)]">of</span>
            <input
              type="number"
              value={form.marksAvailable}
              onChange={(e) => set("marksAvailable")(e.target.value)}
              placeholder="total"
              className="tabular h-8 w-[70px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[12.5px]"
            />
          </div>
        </div>

        {error && <div className="mt-3 text-[12.5px] text-[var(--danger)]">{error}</div>}

        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" disabled={busy || !form.category} onClick={save}>
            Log it
          </Button>
        </div>
      </div>
    </div>
  );
}

const INPUT =
  "w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13px] text-[var(--ink)] h-8 focus:border-[var(--accent)]";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">
        {label}
      </span>
      {children}
    </label>
  );
}

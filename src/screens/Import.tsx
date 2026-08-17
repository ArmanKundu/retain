import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, X } from "lucide-react";

import { AiAction, AiGate, useAi } from "../components/Ai";
import {
  Button,
  Card,
  ColourDot,
  Segmented,
  SectionTitle,
  cx,
} from "../components/ui";
import { api } from "../lib/api";
import type { Delimiter, ImportPreview, ImportResult } from "../lib/types";
import { useApp } from "../store";

/**
 * Paste-to-import, matching Anki's format.
 *
 * Two things this screen refuses to do:
 *
 *   * commit anything you haven't seen — the preview is the whole point, and
 *     the Import button is disabled until there is something real to add; and
 *   * discard a line quietly. Every row that produced no card is listed with
 *     its line number and the reason, because a paste that half-worked and said
 *     nothing is worse than one that failed outright.
 */

const DELIMITERS: { value: Delimiter | "auto"; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "tab", label: "Tab" },
  { value: "semicolon", label: "Semicolon" },
  { value: "comma", label: "Comma" },
];

const PLACEHOLDER_STANDARD = `Front<TAB>Back<TAB>optional tags

Paste straight from Anki, or type your own:

What is a codon?\tThree bases coding for one amino acid\tbio genetics
Transcription happens in the {{c1::nucleus}}`;

const PLACEHOLDER_QUOTE = `Quote<TAB>Source & context<TAB>Theme

"I am fire and air"\tCleopatra, Act V sc ii\tpower and transcendence`;

export function ImportScreen({ onDone }: { onDone: () => void }) {
  const subjects = useApp((s) => s.subjects);
  const setRoute = useApp((s) => s.setRoute);

  const [subjectId, setSubjectId] = useState<number | null>(
    subjects[0]?.id ?? null,
  );
  const [quoteMode, setQuoteMode] = useState(false);
  const [delimiter, setDelimiter] = useState<Delimiter | "auto">("auto");
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const subject = subjects.find((s) => s.id === subjectId);

  // Default English subjects to quote mode — that's the whole reason the card
  // type exists, and having to remember to flip it defeats the point.
  useEffect(() => {
    if (subject) setQuoteMode(subject.subjectType === "english");
  }, [subject?.id, subject?.subjectType]);

  // Preview on a short debounce: fast enough to feel live, slow enough not to
  // reparse on every keystroke of a thousand-line paste.
  useEffect(() => {
    if (!text.trim()) {
      setPreview(null);
      return;
    }
    const t = setTimeout(() => {
      api
        .previewCardImport(
          text,
          delimiter === "auto" ? null : delimiter,
          quoteMode,
        )
        .then(setPreview)
        .catch((e) => setError(String(e)));
    }, 180);
    return () => clearTimeout(t);
  }, [text, delimiter, quoteMode]);

  const breakdown = useMemo(() => {
    if (!preview) return null;
    const counts = { basic: 0, cloze: 0, quote: 0 };
    for (const c of preview.cards) counts[c.noteType] += 1;
    return counts;
  }, [preview]);

  const commit = async () => {
    if (subjectId == null || !preview || preview.cards.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const r = await api.importCards(
        subjectId,
        null,
        text,
        delimiter === "auto" ? null : delimiter,
        quoteMode,
      );
      setResult(r);
      setText("");
      setPreview(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 sm:px-9 pb-14">
      {/* Content scrolls under the title bar. macOS separates the two with a
          hard edge rather than letting text vanish mid-letter. */}
      <div className="titlebar-drag scroll-edge h-11" />

      <header className="mb-6 flex items-center">
        <div>
          <h1 className="text-[24px] font-semibold tracking-[var(--track-display)]">
            Add cards
          </h1>
          <p className="mt-1 text-[13.5px] text-[var(--ink-dim)]">
            Paste in Anki's text format. Nothing is saved until you've seen the
            preview.
          </p>
        </div>
        <Button variant="ghost" className="ml-auto" onClick={onDone}>
          Done
        </Button>
      </header>

      <Card className="p-5">
        <SectionTitle>Subject</SectionTitle>
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {subjects.map((s) => (
            <button
              key={s.id}
              onClick={() => setSubjectId(s.id)}
              className={cx(
                "flex items-center gap-2 rounded-full border px-3 py-1.5 text-[13px] transition-all duration-[120ms] active:scale-[0.97]",
                subjectId === s.id
                  ? "border-[var(--ink-faint)] bg-[var(--surface-hi)] text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)]",
              )}
            >
              <ColourDot colour={s.colour} size={8} />
              {s.name}
            </button>
          ))}
        </div>

        <div className="mt-5 flex flex-wrap items-center gap-4">
          <div>
            <SectionTitle>Card type</SectionTitle>
            <div className="mt-2">
              <Segmented
                size="sm"
                value={quoteMode ? "quote" : "standard"}
                onChange={(v) => setQuoteMode(v === "quote")}
                options={[
                  { value: "standard", label: "Basic / Cloze" },
                  { value: "quote", label: "Quote" },
                ]}
              />
            </div>
          </div>

          <div>
            <SectionTitle>Separator</SectionTitle>
            <div className="mt-2">
              <Segmented
                size="sm"
                value={delimiter}
                onChange={setDelimiter}
                options={DELIMITERS}
              />
            </div>
          </div>
        </div>

        <p className="mt-3 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          {quoteMode
            ? "Quote → source & context → theme. The third column is kept whole, not split into tags."
            : "Front → back → optional tags. {{c1::…}} deletions become one card per distinct number."}
        </p>

        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          spellCheck={false}
          placeholder={quoteMode ? PLACEHOLDER_QUOTE : PLACEHOLDER_STANDARD}
          className="selectable mt-4 h-56 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 font-mono text-[12.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)]"
        />
      </Card>

      {/* Generating cards writes tab-separated rows into the box above rather
          than straight into the deck. Generated cards then go through exactly
          the same parse, preview and confirm as a manual paste — there is no
          path that puts an unreviewed card into your reviews. */}
      {!quoteMode && (
        <NotesToCards
          subjectId={subjectId}
          onGenerated={(tsv) =>
            setText((t) => (t.trim() ? `${t.trim()}\n${tsv}` : tsv))
          }
          onOpenSettings={() => setRoute("settings")}
        />
      )}

      {/* Preview */}
      {preview && (
        <Card className="animate-in mt-4 p-5">
          <div className="flex items-center gap-3">
            <SectionTitle>Preview</SectionTitle>
            <span className="text-[12px] text-[var(--ink-faint)]">
              separator detected: {preview.delimiter}
            </span>
          </div>

          <div className="mt-3 flex flex-wrap gap-4 text-[13px]">
            <span className="text-[var(--ink)]">
              <span className="tabular font-medium">
                {preview.cards.length}
              </span>{" "}
              cards
            </span>
            {breakdown && breakdown.basic > 0 && (
              <span className="text-[var(--ink-dim)]">
                {breakdown.basic} basic
              </span>
            )}
            {breakdown && breakdown.cloze > 0 && (
              <span className="text-[var(--ink-dim)]">
                {breakdown.cloze} cloze
              </span>
            )}
            {breakdown && breakdown.quote > 0 && (
              <span className="text-[var(--ink-dim)]">
                {breakdown.quote} quote
              </span>
            )}
            {preview.skipped.length > 0 && (
              <span className="text-[var(--warn)]">
                {preview.skipped.length} skipped
              </span>
            )}
          </div>

          {preview.cards.length > 0 && (
            <div className="mt-4 max-h-52 space-y-2 overflow-y-auto pr-1">
              {preview.cards.slice(0, 40).map((c, i) => (
                <div
                  key={i}
                  className="rounded-[var(--r-sm)] border border-[var(--line-soft)] bg-[var(--surface-hi)] px-3 py-2"
                >
                  <div className="selectable truncate text-[13px] text-[var(--ink)]">
                    {c.front}
                    {c.clozeIndex !== null && (
                      <span className="ml-2 text-[11px] text-[var(--accent)]">
                        c{c.clozeIndex}
                      </span>
                    )}
                  </div>
                  <div className="selectable mt-0.5 truncate text-[12px] text-[var(--ink-dim)]">
                    {c.back || (
                      <span className="italic text-[var(--ink-faint)]">
                        no back
                      </span>
                    )}
                    {c.extra && (
                      <span className="ml-2 text-[var(--ink-faint)]">
                        · {c.extra}
                      </span>
                    )}
                  </div>
                </div>
              ))}
              {preview.cards.length > 40 && (
                <div className="py-1 text-center text-[12px] text-[var(--ink-faint)]">
                  and {preview.cards.length - 40} more
                </div>
              )}
            </div>
          )}

          {/* Every skipped line, with its reason. Never silent. */}
          {preview.skipped.length > 0 && (
            <div className="mt-4 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--warn)_30%,transparent)] bg-[color-mix(in_srgb,var(--warn)_8%,transparent)] p-3">
              <div className="flex items-center gap-1.5 text-[12.5px] text-[var(--warn)]">
                <AlertTriangle size={13} />
                These lines produced no card
              </div>
              <div className="mt-2 max-h-36 space-y-1.5 overflow-y-auto">
                {preview.skipped.map((s) => (
                  <div
                    key={s.lineNumber}
                    className="text-[12px] leading-relaxed"
                  >
                    <span className="tabular text-[var(--ink-faint)]">
                      line {s.lineNumber}
                    </span>
                    <span className="selectable ml-2 text-[var(--ink-dim)]">
                      {s.text.slice(0, 60)}
                      {s.text.length > 60 && "…"}
                    </span>
                    <div className="text-[var(--ink-faint)]">{s.reason}</div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </Card>
      )}

      {error && (
        <div className="mt-4 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] px-4 py-3 text-[13px] text-[var(--danger)]">
          {error}
        </div>
      )}

      {result && (
        <div className="animate-in mt-4 flex items-center gap-2 rounded-[var(--r-md)] border border-[var(--color-positive)]/30 bg-[var(--color-positive)]/10 px-4 py-3 text-[13px]">
          <Check size={15} className="text-[var(--color-positive)]" />
          <span className="text-[var(--ink)]">
            Added {result.added} {result.added === 1 ? "card" : "cards"}
            {result.duplicates > 0 && (
              <span className="text-[var(--ink-dim)]">
                {" "}
                · skipped {result.duplicates} already in this subject
              </span>
            )}
          </span>
          <button
            onClick={() => setResult(null)}
            className="ml-auto text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            <X size={14} />
          </button>
        </div>
      )}

      <Button
        size="lg"
        variant="primary"
        className="mt-5 w-full"
        disabled={
          busy || subjectId == null || !preview || preview.cards.length === 0
        }
        onClick={commit}
      >
        {busy
          ? "Adding…"
          : preview && preview.cards.length > 0
            ? `Add ${preview.cards.length} ${preview.cards.length === 1 ? "card" : "cards"} to ${subject?.name ?? ""}`
            : "Nothing to add yet"}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------

/**
 * Turn pasted notes into draft cards.
 *
 * The output is written back into the paste box as tab-separated rows, so it
 * lands in the same preview-and-confirm flow as anything typed by hand. That's
 * the whole point: a generated card you never looked at is a card you'll be
 * re-reviewing wrongly for months.
 */
function NotesToCards({
  subjectId,
  onGenerated,
  onOpenSettings,
}: {
  subjectId: number | null;
  onGenerated: (tsv: string) => void;
  onOpenSettings: () => void;
}) {
  const { status } = useAi();
  const [notes, setNotes] = useState("");
  const [count, setCount] = useState(10);

  return (
    <Card className="animate-in mt-4 p-5">
      <SectionTitle>Generate from notes</SectionTitle>

      <div className="mt-2.5">
        <AiGate
          status={status}
          what="turn a page of notes into draft cards"
          onOpenSettings={onOpenSettings}
        >
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            spellCheck={false}
            placeholder="Paste notes, a textbook section, or your own summary…"
            className="selectable h-32 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 text-[12.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)]"
          />

          <div className="mt-3 flex items-center gap-3">
            <label className="text-[12.5px] text-[var(--ink-dim)]">
              At most
            </label>
            <input
              type="number"
              min={1}
              max={40}
              value={count}
              onChange={(e) => setCount(Number(e.target.value))}
              className="tabular h-8 w-16 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[13px] text-[var(--ink)] focus:border-[var(--accent)]"
            />
            <span className="text-[12.5px] text-[var(--ink-dim)]">cards</span>

            <AiAction
              className="ml-auto"
              label="Generate"
              disabled={subjectId == null || notes.trim().length < 40}
              run={() => api.aiCardsFromNotes(subjectId!, notes, count)}
              onDone={(cards) => {
                onGenerated(
                  cards
                    .map(
                      (c) =>
                        `${c.front.replace(/\t/g, " ")}\t${c.back.replace(/\t/g, " ")}`,
                    )
                    .join("\n"),
                );
                setNotes("");
              }}
            />
          </div>

          <p className="mt-3 text-[12px] leading-relaxed text-[var(--ink-faint)]">
            Cards are added to the box above for you to read, edit and confirm —
            nothing goes into your deck until you press Import. Expect to delete
            a few; that's the job.
          </p>
        </AiGate>
      </div>
    </Card>
  );
}

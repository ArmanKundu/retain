import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Check, Layers, LayoutGrid, Plus } from "lucide-react";

import { CardAnswer, CardQuestion } from "../components/CardFace";
import { DeckBrowser, type DeckTarget } from "../components/DeckBrowser";
import { Button, Card, ColourDot, cx } from "../components/ui";
import {
  HintLadder,
  ModePicker,
  WriteAnswer,
  type StudyMode,
} from "../components/StudyModes";
import { api } from "../lib/api";
import type {
  IntervalPreview,
  AnswerResult,
  QueueCounts,
  QueueItem,
  Rating,
} from "../lib/types";
import { useApp } from "../store";

/**
 * The review screen.
 *
 * The division of labour matters here: this component decides *what to show*,
 * and the Rust backend decides *when the card comes back*. Every rating is sent
 * to `answer_card`, and the interval displayed afterwards is whatever FSRS
 * returned. There is no scheduling arithmetic in this file — if there were, it
 * would inevitably drift from the backend's.
 */

const RATINGS: { rating: Rating; label: string; key: string; tone: string }[] =
  [
    {
      rating: "again",
      label: "Again",
      key: "1",
      tone: "text-[var(--danger)] border-[color-mix(in_srgb,var(--danger)_35%,transparent)] hover:border-[var(--danger)]",
    },
    {
      rating: "hard",
      label: "Hard",
      key: "2",
      tone: "text-[var(--warn)] border-[color-mix(in_srgb,var(--warn)_35%,transparent)] hover:border-[var(--warn)]",
    },
    {
      rating: "good",
      label: "Good",
      key: "3",
      tone: "text-[var(--color-positive)] border-[var(--color-positive)]/35 hover:border-[var(--color-positive)]",
    },
    {
      rating: "easy",
      label: "Easy",
      key: "4",
      tone: "text-[var(--accent)] border-[var(--accent)]/35 hover:border-[var(--accent)]",
    },
  ];

/**
 * Describe when a card comes back, from what the backend returned.
 *
 * Formatting only — the values are FSRS's. Intraday steps report minutes
 * derived from `dueAt` rather than a made-up figure.
 */
function nextLabel(result: AnswerResult): string {
  if (result.intervalDays !== null) {
    const d = result.intervalDays;
    if (d < 30) return `${d} ${d === 1 ? "day" : "days"}`;
    if (d < 365) return `${Math.round(d / 30)} months`;
    return `${(d / 365).toFixed(1)} years`;
  }
  const minutes = Math.max(
    1,
    Math.round((new Date(result.dueAt).getTime() - Date.now()) / 60000),
  );
  return minutes < 60 ? `${minutes} min` : `${Math.round(minutes / 60)} h`;
}

const STATE_LABEL: Record<string, string> = {
  new: "New",
  learning: "Learning",
  review: "Review",
  relearning: "Relearning",
};

export function Review({ onImport }: { onImport: () => void }) {
  const subjects = useApp((s) => s.subjects);

  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [index, setIndex] = useState(0);
  const [counts, setCounts] = useState<QueueCounts | null>(null);
  const [revealed, setRevealed] = useState(false);
  // The mode persists across cards within a session — switching it per card
  // would make it a chore rather than a choice.
  const [mode, setMode] = useState<StudyMode>("flip");
  const [subjectFilter, setSubjectFilter] = useState<number | null>(null);
  // Browse first, study second. The queue answers "what's due"; browsing
  // answers "do I know Genetics", which is the question the week before a SAC.
  const [browsing, setBrowsing] = useState(false);
  /**
   * A practice run, or null for the ordinary scheduled queue.
   *
   * Practice never calls `answerCard`. Answering early through the real queue
   * tells FSRS you needed the card early and shortens every interval in
   * response, so a week of cramming would leave the schedule permanently
   * worse — see `cards::practice`.
   */
  const [practice, setPractice] = useState<DeckTarget | null>(null);
  const [last, setLast] = useState<{ rating: Rating; next: string } | null>(
    null,
  );
  const [intervals, setIntervals] = useState<IntervalPreview[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // When the current card was put on screen. Sent with the rating so the review
  // log records real thinking time instead of a fabricated zero.
  const presentedAt = useRef<string>(new Date().toISOString());

  const card = queue[index];

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [items, c] = await Promise.all([
        practice
          ? api.practiceQueue(practice.subjectId, practice.topicId, 40)
          : api.reviewQueue(subjectFilter, 200),
        api.reviewCounts(subjectFilter),
      ]);
      setQueue(items);
      setCounts(c);
      setIndex(0);
      setRevealed(false);
      presentedAt.current = new Date().toISOString();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [subjectFilter, practice]);

  useEffect(() => {
    void load();
  }, [load]);

  const reveal = useCallback(() => {
    if (card && !revealed) setRevealed(true);
  }, [card, revealed]);

  // The next interval for each rating, shown under the buttons so the schedule
  // is legible without exposing FSRS. Fetched on reveal rather than on load so
  // it never sits on the path between seeing a card and answering it.
  useEffect(() => {
    if (!card || !revealed) {
      setIntervals(null);
      return;
    }
    let live = true;
    void api
      .previewIntervals(card.cardId)
      .then((p) => live && setIntervals(p))
      // A missing preview costs a hint, not the review. The buttons still work.
      .catch(() => live && setIntervals(null));
    return () => {
      live = false;
    };
  }, [card?.cardId, revealed]);

  const answer = useCallback(
    async (rating: Rating) => {
      if (!card || !revealed || busy) return;
      setBusy(true);
      try {
        // Practice reads and never writes: no scheduling, no review log, and so
        // no way for a cramming session to manufacture a streak day either.
        if (practice) {
          setLast(null);
        } else {
          const result = await api.answerCard(
            card.cardId,
            rating,
            presentedAt.current,
          );
          setLast({ rating, next: nextLabel(result) });
        }

        const nextIndex = index + 1;
        if (nextIndex >= queue.length) {
          // Batch exhausted. Refetching also picks up learning cards whose
          // short step has come due again while we worked.
          await load();
        } else {
          setIndex(nextIndex);
          setRevealed(false);
          presentedAt.current = new Date().toISOString();
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [card, revealed, busy, index, queue.length, load, practice],
  );

  // Keyboard first: space/enter to reveal, 1–4 to rate. Reaching for the mouse
  // between every card is what makes a review session feel slow.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;

      // Space reveals in flip mode only. In write and hint mode the whole
      // point is the step before the answer, and a shortcut that skips it
      // turns them back into flip with extra clicks.
      if (
        !revealed &&
        mode === "flip" &&
        (e.code === "Space" || e.code === "Enter")
      ) {
        e.preventDefault();
        reveal();
        return;
      }
      if (revealed) {
        const hit = RATINGS.find((r) => r.key === e.key);
        if (hit) {
          e.preventDefault();
          void answer(hit.rating);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [revealed, reveal, answer, mode]);

  const totalWaiting = useMemo(
    () => (counts ? counts.dueReviews + counts.newAvailable : 0),
    [counts],
  );

  return (
    <div className="mx-auto flex h-full max-w-[760px] flex-col px-9 pb-10">
      <div className="titlebar-drag h-11 shrink-0" />

      {/* Header: counts and filter */}
      <header className="mb-6 flex shrink-0 items-center gap-4">
        <div className="flex items-baseline gap-3">
          <h1 className="text-[24px] font-semibold tracking-[var(--track-display)]">
            Review
          </h1>
          {counts && (
            <span className="tabular text-[13px] text-[var(--ink-dim)]">
              {counts.dueReviews} due
              <span className="mx-1.5 text-[var(--ink-faint)]">·</span>
              {counts.newAvailable} new
            </span>
          )}
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => {
              setBrowsing((b) => !b);
              setPractice(null);
            }}
            aria-pressed={browsing}
            title="Browse your decks by subject and topic"
            className={cx(
              "mr-1 flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12px] transition-colors",
              browsing
                ? "border-[var(--accent)]/40 bg-[var(--accent)]/12 text-[var(--accent)]"
                : "border-[var(--line)] text-[var(--ink-faint)] hover:border-[var(--ink-faint)]",
            )}
          >
            <LayoutGrid size={12} />
            Browse
          </button>

          <button
            onClick={() => setSubjectFilter(null)}
            className={cx(
              "rounded-full border px-2.5 py-1 text-[12px] transition-colors",
              subjectFilter === null
                ? "border-[var(--ink-faint)] text-[var(--ink)]"
                : "border-[var(--line)] text-[var(--ink-faint)] hover:border-[var(--ink-faint)]",
            )}
          >
            All
          </button>
          {subjects.map((s) => (
            <button
              key={s.id}
              onClick={() => setSubjectFilter(s.id)}
              className={cx(
                "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12px] transition-colors",
                subjectFilter === s.id
                  ? "border-[var(--ink-faint)] text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-faint)] hover:border-[var(--ink-faint)]",
              )}
            >
              <ColourDot colour={s.colour} size={7} />
              {s.name}
            </button>
          ))}
        </div>
      </header>

      {error && (
        <div className="mb-4 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] px-4 py-3 text-[13px] text-[var(--danger)]">
          {error}
        </div>
      )}

      {browsing && (
        <DeckBrowser
          onStudy={(target, studyMode) => {
            setBrowsing(false);
            setSubjectFilter(target.subjectId);
            setPractice(studyMode === "practice" ? target : null);
          }}
        />
      )}

      {/* Card */}
      {browsing ? null : loading ? (
        <div className="flex-1" />
      ) : card ? (
        <div className="relative flex flex-1 flex-col">
          {/* The pile behind the card.
              Showing one card at a time hides the only progress signal that
              matters mid-session. Two sheets is enough to read as "a stack" —
              more just looks like a shadow — and they disappear on the last
              two cards, so the pile visibly runs out. */}
          {[2, 1].map((depth) =>
            queue.length - index > depth ? (
              <div
                key={depth}
                aria-hidden
                className="card-stack-sheet"
                style={{
                  transform: `translateY(${depth * 7}px) scale(${1 - depth * 0.018})`,
                  opacity: 0.5 / depth,
                }}
              />
            ) : null,
          )}

          <Card
            key={card.cardId}
            elevation="raised"
            className="animate-pop relative flex flex-1 flex-col rounded-[var(--r-xl)] p-8"
          >
            {/* How far through the batch you are. A thin line rather than a
              figure: it's peripheral information and shouldn't ask to be read. */}
            <div
              aria-hidden
              className="absolute inset-x-0 top-0 h-[2px] overflow-hidden rounded-t-[var(--r-xl)]"
            >
              <div
                className="h-full rounded-r-full transition-[width] duration-[var(--t-slow)] ease-[var(--ease)]"
                style={{
                  width: `${((index + (revealed ? 1 : 0)) / Math.max(1, queue.length)) * 100}%`,
                  background: card.colour,
                }}
              />
            </div>

            <div className="flex shrink-0 items-center gap-2 text-[12.5px] text-[var(--ink-faint)]">
              <ColourDot colour={card.colour} size={8} />
              <span>{card.subjectName}</span>
              <span>·</span>
              <span>{STATE_LABEL[card.state] ?? card.state}</span>
              {practice && (
                <span className="rounded-full bg-[var(--surface-hi)] px-2 py-0.5 text-[11px] text-[var(--ink-dim)]">
                  Practice — nothing is scheduled
                </span>
              )}
              {card.noteType === "cloze" && <span>· Cloze</span>}
              {card.noteType === "quote" && <span>· Quote</span>}
              <div className="ml-auto flex items-center gap-3">
                <ModePicker mode={mode} onChange={setMode} />
                <span className="tabular">
                  {index + 1} / {queue.length}
                </span>
              </div>
            </div>

            <div className="mt-7 flex flex-1 flex-col justify-center">
              <CardQuestion card={card} />

              {/* The answer turns into place rather than fading in — a card has
                two sides, and a crossfade makes it a div whose contents
                changed. Reduced motion gets the state change without the
                rotation. */}
              {revealed && (
                <div className="flip-scene mt-7 border-t border-[var(--line-soft)] pt-7">
                  <div className="flip-inner is-flipped">
                    <div className="flip-face is-back">
                      <CardAnswer card={card} />
                    </div>
                    {/* The front face is what the rotation turns away from; it
                      holds the layout height while the back is absolute. */}
                    <div className="flip-face invisible" aria-hidden>
                      <CardAnswer card={card} />
                    </div>
                  </div>
                </div>
              )}
            </div>

            <div className="mt-7 shrink-0">
              {!revealed ? (
                mode === "write" ? (
                  <WriteAnswer
                    key={card.cardId}
                    expected={card.back}
                    onSubmitted={reveal}
                  />
                ) : mode === "hint" ? (
                  <HintLadder
                    key={card.cardId}
                    answer={card.back}
                    onExhausted={reveal}
                  />
                ) : (
                  <Button
                    size="lg"
                    variant="primary"
                    className="w-full"
                    onClick={reveal}
                  >
                    Reveal answer
                    <span className="ml-1 text-[11px] opacity-60">Space</span>
                  </Button>
                )
              ) : (
                <div className="animate-rise grid grid-cols-4 gap-2">
                  {RATINGS.map((r) => {
                    const preview = intervals?.find(
                      (i) => i.rating === r.rating,
                    );
                    return (
                      <button
                        key={r.rating}
                        disabled={busy}
                        onClick={() => void answer(r.rating)}
                        aria-keyshortcuts={r.key}
                        className={cx(
                          "pressable group flex h-[54px] flex-col items-center justify-center gap-0.5",
                          "rounded-[var(--r-md)] border bg-[var(--surface-hi)]/70",
                          "hover:shadow-[var(--e-sm)] disabled:opacity-40",
                          r.tone,
                        )}
                      >
                        <span className="flex items-center gap-1.5 text-[13px] font-medium">
                          {r.label}
                          <span className="text-[10px] opacity-45">
                            {r.key}
                          </span>
                        </span>
                        {/* Reserved height so the row doesn't jump when the
                          preview arrives a moment after reveal. */}
                        <span className="tabular h-[13px] text-[11px] text-[var(--ink-faint)]">
                          {preview ? intervalLabel(preview.intervalDays) : ""}
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </Card>
        </div>
      ) : (
        <Card className="flex flex-1 items-center justify-center">
          {counts &&
          counts.newRemainingTotal === 0 &&
          counts.dueReviews === 0 ? (
            <div className="flex flex-col items-center px-6 py-14 text-center">
              <Layers size={22} className="mb-3 text-[var(--ink-faint)]" />
              <div className="text-[14px] font-medium text-[var(--ink-dim)]">
                No cards yet
              </div>
              <div className="mt-1.5 max-w-[380px] text-[13px] leading-relaxed text-[var(--ink-faint)]">
                Paste cards in from Anki, or write your own. One good atomic
                card beats fifty rushed ones.
              </div>
              <Button className="mt-5" onClick={onImport}>
                <Plus size={15} />
                Add cards
              </Button>
            </div>
          ) : (
            <div className="flex flex-col items-center px-6 py-14 text-center">
              <Check size={22} className="mb-3 text-[var(--color-positive)]" />
              <div className="text-[14px] font-medium text-[var(--ink-dim)]">
                Nothing due right now
              </div>
              <div className="mt-1.5 max-w-[400px] text-[13px] leading-relaxed text-[var(--ink-faint)]">
                {counts && counts.newRemainingTotal > 0 ? (
                  <>
                    {counts.newIntroducedToday} new cards introduced today.
                    There are {counts.newRemainingTotal} more waiting — they'll
                    be offered tomorrow, a subject at a time, so the review load
                    stays manageable.
                  </>
                ) : (
                  <>Everything scheduled for today is done.</>
                )}
              </div>
            </div>
          )}
        </Card>
      )}

      {/* What the backend just decided */}
      <div className="mt-3 h-5 shrink-0 text-center text-[12.5px] text-[var(--ink-faint)]">
        {last && (
          <span className="animate-in">
            {RATINGS.find((r) => r.rating === last.rating)?.label} → next in{" "}
            {last.next}
          </span>
        )}
        {!last && totalWaiting > 0 && card && (
          <span>Space to reveal, then 1–4 to rate</span>
        )}
      </div>
    </div>
  );
}

/**
 * How far out a rating schedules a card, in the shortest honest form.
 *
 * `null` means an intraday learning step, which FSRS measures in minutes rather
 * than days — "<10m" is truer there than rounding to "0d".
 */
function intervalLabel(days: number | null): string {
  if (days == null) return "<10m";
  if (days <= 0) return "today";
  if (days === 1) return "1d";
  if (days < 30) return `${days}d`;
  if (days < 365) return `${Math.round(days / 30)}mo`;
  return `${(days / 365).toFixed(1)}y`;
}

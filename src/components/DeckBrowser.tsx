// Browsing your decks, rather than being handed the next card.
//
// Review only ever answered "what's due now". That is the right question at 8pm
// on a Tuesday and the wrong one in the week before a SAC, when what you need is
// to open Genetics specifically and find out whether you know it.
//
// Three levels: subjects → topics → one deck. Each level shows the same four
// numbers, so the shape of the thing you're looking at doesn't change as you go
// down.
//
// The number that leads is **mastery**, and it is deliberately not
// percent-correct. A card you answered right twice this morning scores 100% and
// will be gone by Thursday. Mastery counts cards whose FSRS stability is a
// fortnight or more — it moves slowly because that's what learning does, and a
// metric that jumps after one good session is a metric that lies.

import { useCallback, useEffect, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Dumbbell,
  Play,
  TriangleAlert,
} from "lucide-react";

import { api } from "../lib/api";
import type {
  DeckStats,
  Strength,
  SubjectMastery,
  TopicMastery,
} from "../lib/types";
import { Button, Card } from "./ui";
import { SectionHeader } from "./primitives";

export type DeckTarget = {
  subjectId: number;
  topicId: number | null;
  label: string;
};

/** A ring showing what share of a deck will survive a fortnight. */
function MasteryRing({
  value,
  colour,
  size = 46,
}: {
  value: number;
  colour: string;
  size?: number;
}) {
  const stroke = 4;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;

  return (
    <svg width={size} height={size} className="shrink-0" aria-hidden>
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke="var(--line)"
        strokeWidth={stroke}
      />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke={colour}
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - Math.max(0, Math.min(1, value)))}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        style={{ transition: "stroke-dashoffset var(--t-slow) var(--ease)" }}
      />
      <text
        x="50%"
        y="52%"
        dominantBaseline="middle"
        textAnchor="middle"
        className="fill-[var(--ink-dim)] text-[10px] tabular-nums"
      >
        {Math.round(value * 100)}
      </text>
    </svg>
  );
}

/** New / Learning / Mastered as one bar. The shape is the information. */
function StrengthBar({ s, colour }: { s: Strength; colour: string }) {
  if (s.total === 0) return null;
  const pct = (n: number) => `${(n / s.total) * 100}%`;

  return (
    <div className="flex h-1.5 w-full overflow-hidden rounded-full bg-[var(--line-soft)]">
      <div style={{ width: pct(s.mastered), background: colour }} />
      <div style={{ width: pct(s.learning), background: `${colour}66` }} />
      <div style={{ width: pct(s.new), background: "var(--line)" }} />
    </div>
  );
}

function dueLabel(s: Strength): string {
  if (s.total === 0) return "No cards yet";
  if (s.dueToday > 0) return `${s.dueToday} due`;
  if (!s.nextDueOn) return "Nothing scheduled";

  const days = Math.round(
    (new Date(`${s.nextDueOn}T12:00:00`).getTime() -
      new Date().setHours(12, 0, 0, 0)) /
      86400000,
  );
  if (days <= 0) return "Due now";
  if (days === 1) return "Next tomorrow";
  return `Next in ${days} days`;
}

export function DeckBrowser({
  onStudy,
}: {
  /** Fired with the deck and whether the schedule should be updated. */
  onStudy: (target: DeckTarget, mode: "review" | "practice") => void;
}) {
  const [subjects, setSubjects] = useState<SubjectMastery[]>([]);
  const [openSubject, setOpenSubject] = useState<SubjectMastery | null>(null);
  const [topics, setTopics] = useState<TopicMastery[]>([]);
  const [deck, setDeck] = useState<{
    target: DeckTarget;
    stats: DeckStats;
  } | null>(null);

  useEffect(() => {
    void api
      .subjectMastery()
      .then(setSubjects)
      .catch(() => setSubjects([]));
  }, []);

  const openTopics = useCallback(async (s: SubjectMastery) => {
    setOpenSubject(s);
    setDeck(null);
    setTopics(await api.topicMastery(s.subjectId).catch(() => []));
  }, []);

  const openDeck = useCallback(async (target: DeckTarget) => {
    const stats = await api
      .deckStats(target.subjectId, target.topicId)
      .catch(() => null);
    if (stats) setDeck({ target, stats });
  }, []);

  // --- level 3: one deck ---------------------------------------------------
  if (deck && openSubject) {
    return (
      <DeckDashboard
        target={deck.target}
        stats={deck.stats}
        colour={openSubject.colour}
        onBack={() => setDeck(null)}
        onStudy={onStudy}
      />
    );
  }

  // --- level 2: topics inside a subject ------------------------------------
  if (openSubject) {
    return (
      <section className="animate-rise">
        <button
          onClick={() => setOpenSubject(null)}
          className="pressable mb-3 flex items-center gap-1 text-[12.5px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
        >
          <ChevronLeft size={13} />
          All subjects
        </button>

        <SectionHeader
          title={openSubject.name}
          hint={`${openSubject.total} cards · ${Math.round(openSubject.mastery * 100)}% mastered`}
        >
          <Button
            size="sm"
            variant="primary"
            onClick={() =>
              onStudy(
                {
                  subjectId: openSubject.subjectId,
                  topicId: null,
                  label: openSubject.name,
                },
                "review",
              )
            }
          >
            <Play size={13} />
            Review all
          </Button>
        </SectionHeader>

        <div className="space-y-1.5">
          {topics.map((t) => (
            <button
              key={t.topicId ?? "none"}
              onClick={() =>
                void openDeck({
                  subjectId: openSubject.subjectId,
                  topicId: t.topicId,
                  label: `${openSubject.name} · ${t.name}`,
                })
              }
              className="flex w-full items-center gap-4 rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface)] px-4 py-3 text-left transition-colors duration-[var(--t-fast)] hover:border-[var(--line)]"
            >
              <MasteryRing
                value={t.mastery}
                colour={openSubject.colour}
                size={38}
              />

              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <span className="truncate text-[14px] text-[var(--ink)]">
                    {t.name}
                  </span>
                  <span className="shrink-0 text-[11.5px] text-[var(--ink-faint)]">
                    {t.total} cards
                  </span>
                </div>
                <div className="mt-1.5">
                  <StrengthBar s={t} colour={openSubject.colour} />
                </div>
              </div>

              <span className="shrink-0 text-[12px] text-[var(--ink-faint)]">
                {dueLabel(t)}
              </span>
              <ChevronRight
                size={14}
                className="shrink-0 text-[var(--ink-faint)]"
              />
            </button>
          ))}
        </div>
      </section>
    );
  }

  // --- level 1: every subject ----------------------------------------------
  return (
    <section className="animate-rise">
      <SectionHeader
        title="Your decks"
        hint="mastery is cards that will last a fortnight"
      />

      {subjects.length === 0 ? (
        <p className="px-1 text-[13.5px] leading-relaxed text-[var(--ink-dim)]">
          No subjects yet.
        </p>
      ) : (
        <div className="grid gap-2.5 sm:grid-cols-2">
          {subjects.map((s) => (
            <button
              key={s.subjectId}
              onClick={() => void openTopics(s)}
              className="flex items-center gap-4 rounded-[var(--r-lg)] border border-[var(--line-soft)] bg-[var(--surface)] p-4 text-left transition-colors duration-[var(--t-fast)] hover:border-[var(--line)]"
            >
              <MasteryRing value={s.mastery} colour={s.colour} />

              <div className="min-w-0 flex-1">
                <div className="truncate text-[14.5px] text-[var(--ink)]">
                  {s.name}
                </div>
                <div className="mt-0.5 text-[12px] text-[var(--ink-faint)]">
                  {dueLabel(s)}
                  {s.leeches > 0 && ` · ${s.leeches} stuck`}
                </div>
                <div className="mt-2">
                  <StrengthBar s={s} colour={s.colour} />
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

function DeckDashboard({
  target,
  stats,
  colour,
  onBack,
  onStudy,
}: {
  target: DeckTarget;
  stats: DeckStats;
  colour: string;
  onBack: () => void;
  onStudy: (target: DeckTarget, mode: "review" | "practice") => void;
}) {
  return (
    <section className="animate-rise">
      <button
        onClick={onBack}
        className="pressable mb-3 flex items-center gap-1 text-[12.5px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
      >
        <ChevronLeft size={13} />
        Back
      </button>

      <Card className="p-5">
        <div className="flex items-center gap-4">
          <MasteryRing value={stats.mastery} colour={colour} size={58} />
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-[17px] font-semibold tracking-[-0.01em]">
              {target.label}
            </h2>
            <p className="mt-0.5 text-[12.5px] text-[var(--ink-dim)]">
              {stats.total} cards · {dueLabel(stats)}
            </p>
          </div>
        </div>

        <div className="mt-4 grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-4">
          <Stat label="New" value={stats.new} />
          <Stat label="Learning" value={stats.learning} />
          <Stat label="Mastered" value={stats.mastered} accent={colour} />
          <Stat
            label="Recent accuracy"
            /* Null is not 0%. "You haven't started" and "you got everything
               wrong" are opposite facts and must not share a rendering. */
            value={
              stats.recentAccuracy === null
                ? "—"
                : `${Math.round(stats.recentAccuracy * 100)}%`
            }
          />
        </div>

        {stats.averageStability !== null && (
          <p className="mt-3 text-[12px] leading-relaxed text-[var(--ink-faint)]">
            Average memory strength {Math.round(stats.averageStability)} days —
            how long you'd hold a typical card in here without seeing it again.
          </p>
        )}

        {stats.leeches > 0 && (
          <div className="mt-3 flex items-start gap-2 rounded-[var(--r-sm)] border border-[var(--warn)]/30 bg-[var(--warn)]/8 px-3 py-2.5">
            <TriangleAlert
              size={13}
              className="mt-0.5 shrink-0 text-[var(--warn)]"
            />
            <p className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
              {stats.leeches} {stats.leeches === 1 ? "card has" : "cards have"}{" "}
              been forgotten eight times or more. That usually means the card is
              badly worded rather than that you aren't trying — worth rewriting
              rather than repeating.
            </p>
          </div>
        )}

        <Heatmap days={stats.recent} colour={colour} />

        <div className="mt-4 flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="primary"
            disabled={stats.dueToday === 0}
            onClick={() => onStudy(target, "review")}
          >
            <Play size={13} />
            {stats.dueToday > 0 ? `Review ${stats.dueToday}` : "Nothing due"}
          </Button>
          <Button
            size="sm"
            disabled={stats.total === 0}
            onClick={() => onStudy(target, "practice")}
          >
            <Dumbbell size={13} />
            Practice
          </Button>
        </div>

        <p className="mt-3 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
          Practice goes through the weakest cards without touching the schedule.
          Answering early through Review would tell the algorithm you needed
          them early and shorten every interval in response, so a week of
          cramming would leave your schedule worse than it started.
        </p>
      </Card>
    </section>
  );
}

function Stat({
  label,
  value,
  accent,
}: {
  label: string;
  value: number | string;
  accent?: string;
}) {
  return (
    <div>
      <div
        className="text-[19px] tabular-nums"
        style={accent ? { color: accent } : undefined}
      >
        {value}
      </div>
      <div className="text-[11.5px] text-[var(--ink-faint)]">{label}</div>
    </div>
  );
}

/** A month of answering. Height is volume, colour is accuracy. */
function Heatmap({
  days,
  colour,
}: {
  days: { date: string; reviews: number; accuracy: number }[];
  colour: string;
}) {
  const busiest = Math.max(1, ...days.map((d) => d.reviews));

  return (
    <div className="mt-4">
      <div className="mb-1.5 text-[11.5px] text-[var(--ink-faint)]">
        Last 30 days
      </div>
      <div className="flex items-end gap-[3px]" style={{ height: 34 }}>
        {days.map((d) => (
          <div
            key={d.date}
            title={
              d.reviews === 0
                ? `${d.date}: nothing`
                : `${d.date}: ${d.reviews} reviews, ${Math.round(d.accuracy * 100)}% recalled`
            }
            className="flex-1 rounded-[var(--r-xs)]"
            style={{
              // A day with no reviews is a visible floor rather than nothing,
              // so gaps read as gaps instead of as the chart ending.
              height:
                d.reviews === 0
                  ? 3
                  : `${Math.max(12, (d.reviews / busiest) * 100)}%`,
              background: d.reviews === 0 ? "var(--line-soft)" : colour,
              opacity: d.reviews === 0 ? 1 : 0.35 + d.accuracy * 0.65,
            }}
          />
        ))}
      </div>
    </div>
  );
}

export { MasteryRing };

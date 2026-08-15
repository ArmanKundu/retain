// Today.
//
// A personal home screen, not a dashboard. The question it answers is "what
// should I do now?", and it answers it before you've read a word — which is why
// the largest object on the page is the next action, not a statistic.
//
// The previous version led with a huge `0m`, which on a quiet morning meant the
// first thing the app said to you was "you have done nothing". Figures are still
// here, but they sit under the action rather than in front of it.
//
// Every number comes from an existing command. Nothing here computes its own
// statistics, and nothing is invented to fill space.

import { useEffect, useState } from "react";
import { ArrowRight, Flame, Layers, Play, Shield } from "lucide-react";

import { DayPlan } from "../components/DayPlan";
import { GoalRing } from "../components/GoalRing";
import { UpcomingEvents } from "../components/UpcomingEvents";
import { SectionHeader, SubjectPill } from "../components/primitives";
import { Button, cx } from "../components/ui";
import { api } from "../lib/api";
import type { Assessment, QueueCounts } from "../lib/types";
import { duration, greeting, timeOfDay } from "../lib/format";
import { useApp, type Route } from "../store";

export function Today() {
  const { boot, streak, rings, recent, subjects, setRoute, timer } = useApp();

  const [counts, setCounts] = useState<QueueCounts | null>(null);
  const [assessments, setAssessments] = useState<Assessment[]>([]);

  useEffect(() => {
    // Both are optional context — a failure costs the line, not the screen.
    void api
      .reviewCounts()
      .then(setCounts)
      .catch(() => setCounts(null));
    void api
      .listAssessments(false)
      .then(setAssessments)
      .catch(() => setAssessments([]));
  }, []);

  const activeMinutes = streak?.todayActiveMinutes ?? 0;
  const threshold = streak?.thresholdMinutes ?? 20;
  const dayProgress = Math.min(
    1,
    threshold > 0 ? activeMinutes / threshold : 0,
  );
  const soon = assessments.filter((a) => a.daysAway >= 0 && a.daysAway <= 14);

  const next = nextAction({
    counts,
    assessments: soon,
    timer: !!timer,
    activeMinutes,
  });

  return (
    <div className="mx-auto w-full max-w-[min(1100px,100%)] px-6 pb-16 sm:px-10">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-7">
        <h1 className="text-[30px] font-semibold tracking-[-0.03em]">
          {greeting(boot?.userName ?? "")}
        </h1>
        <p className="mt-1.5 text-[15px] leading-relaxed text-[var(--ink-dim)]">
          {openingLine(
            activeMinutes,
            streak?.todayQualified ?? false,
            threshold,
          )}
        </p>
      </header>

      {/* The next best action.
          Deliberately the largest object on the page. A study app's job at
          09:00 is to answer one question, and a row of statistics doesn't. */}
      <section className="animate-rise relative mb-4 overflow-hidden rounded-[var(--r-xl)] border border-[var(--line-soft)] bg-[color-mix(in_srgb,var(--surface)_88%,transparent)] shadow-[var(--e-md)]">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0"
          style={{
            background:
              "radial-gradient(80% 130% at 6% 0%, color-mix(in srgb, var(--accent) 11%, transparent) 0%, transparent 60%), radial-gradient(60% 110% at 98% 100%, color-mix(in srgb, var(--color-positive) 9%, transparent) 0%, transparent 58%)",
          }}
        />

        <div className="relative flex flex-wrap items-center gap-x-8 gap-y-5 p-7">
          <div className="min-w-[260px] flex-1">
            <div className="text-[12px] font-medium uppercase tracking-[0.07em] text-[var(--ink-faint)]">
              {next.eyebrow}
            </div>
            <h2 className="mt-2 text-[24px] font-semibold leading-tight tracking-[-0.02em]">
              {next.title}
            </h2>
            <p className="mt-1.5 text-[13.5px] leading-relaxed text-[var(--ink-dim)]">
              {next.detail}
            </p>

            <Button
              size="lg"
              variant="primary"
              className="mt-5"
              onClick={() => setRoute(next.route)}
            >
              {next.icon}
              {next.action}
              <ArrowRight size={15} className="opacity-70" />
            </Button>
          </div>

          {/* Today's progress toward being earned. A ring rather than a bar:
              the day is a threshold that's met, not a score to maximise. */}
          <DayRing
            qualified={streak?.todayQualified ?? false}
            progress={dayProgress}
            minutes={activeMinutes}
          />
        </div>
      </section>

      {/* Supporting figures, quiet by design. */}
      <div className="animate-rise mb-9 flex flex-wrap items-center gap-x-7 gap-y-3 px-1">
        <Figure
          value={
            activeMinutes >= 60
              ? duration(activeMinutes * 60)
              : `${activeMinutes}m`
          }
          label="focused today"
        />
        {counts && counts.dueReviews + counts.newRemainingTotal > 0 && (
          <Figure
            value={counts.dueReviews + counts.newRemainingTotal}
            label={counts.dueReviews > 0 ? "cards waiting" : "new cards ready"}
            onClick={() => setRoute("review")}
          />
        )}
        {soon.length > 0 && (
          <Figure
            value={soon.length}
            label={soon.length === 1 ? "assessment soon" : "assessments soon"}
            onClick={() => setRoute("assessments")}
          />
        )}
        {streak && streak.current > 0 && (
          <Figure
            value={streak.current}
            label={streak.current === 1 ? "day running" : "days running"}
            accent="var(--warn)"
            icon={
              <Flame size={15} strokeWidth={2} className="text-[var(--warn)]" />
            }
          />
        )}
        {streak && streak.freezesAvailable > 0 && (
          <Figure
            value={streak.freezesAvailable}
            label={
              streak.freezesAvailable === 1 ? "freeze ready" : "freezes ready"
            }
            icon={
              <Shield
                size={14}
                strokeWidth={2}
                className="text-[var(--ink-faint)]"
              />
            }
          />
        )}
      </div>

      <DayPlan subjects={subjects} />

      {rings.length > 0 && (
        <section className="animate-rise mb-9">
          <SectionHeader title="This week" hint="hours against your goals" />
          <div className="flex flex-wrap gap-x-9 gap-y-6 px-1">
            {rings.map((r) => (
              <GoalRing key={r.subjectId} ring={r} />
            ))}
          </div>
        </section>
      )}

      <UpcomingEvents />

      <section className="animate-rise">
        <SectionHeader title="Recent sessions" />
        {recent.length === 0 ? (
          <div className="rounded-[var(--r-lg)] border border-dashed border-[var(--line)] px-6 py-10 text-center">
            <div className="text-[14px] font-medium text-[var(--ink-dim)]">
              Nothing logged yet
            </div>
            <p className="mx-auto mt-1.5 max-w-[400px] text-[13px] leading-relaxed text-[var(--ink-faint)]">
              Sessions appear here once you've finished one, with whatever you
              noted at the time.
            </p>
          </div>
        ) : (
          <div className="-mx-3">
            {recent.map((s) => (
              <div
                key={s.id}
                className="flex items-start gap-3 rounded-[var(--r-md)] px-3 py-3 transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]"
              >
                <div className="mt-[5px]">
                  <SubjectPill name={s.subjectName} colour={s.colour} dotOnly />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-2">
                    <span className="text-[13.5px] text-[var(--ink)]">
                      {s.subjectName}
                    </span>
                    {s.topicName && (
                      <span className="truncate text-[12px] text-[var(--ink-faint)]">
                        {s.topicName}
                      </span>
                    )}
                  </div>
                  {s.note && (
                    <div className="selectable mt-0.5 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
                      {s.note}
                    </div>
                  )}
                  {s.pauseCount > 0 && (
                    <div className="mt-0.5 text-[11.5px] text-[var(--ink-faint)]">
                      {s.pauseCount} {s.pauseCount === 1 ? "pause" : "pauses"}
                      {s.idlePauseCount > 0 &&
                        ` · ${s.idlePauseCount} from inactivity`}
                    </div>
                  )}
                </div>

                <div className="shrink-0 text-right">
                  <div className="tabular text-[13px] text-[var(--ink)]">
                    {duration(s.activeSeconds)}
                  </div>
                  <div className="text-[11px] text-[var(--ink-faint)]">
                    {timeOfDay(s.startedAt)}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

// ---------------------------------------------------------------------------

/**
 * What to put in front of the user right now.
 *
 * Ordered by what actually decays: reviews have a due date and get harder the
 * longer they wait, so they outrank everything. An assessment inside three days
 * outranks a general session. With nothing pressing, the suggestion is simply
 * to start — which is the honest answer, and better than manufacturing urgency.
 */
function nextAction({
  counts,
  assessments,
  timer,
  activeMinutes,
}: {
  counts: QueueCounts | null;
  assessments: Assessment[];
  timer: boolean;
  activeMinutes: number;
}): {
  eyebrow: string;
  title: string;
  detail: string;
  action: string;
  route: Route;
  icon: React.ReactNode;
} {
  if (timer) {
    return {
      eyebrow: "In progress",
      title: "You're in a session",
      detail:
        "The dock at the bottom keeps the clock while you work anywhere in Retain.",
      action: "Open the timer",
      route: "timer",
      icon: <Play size={16} />,
    };
  }

  const due = counts?.dueReviews ?? 0;
  if (due > 0) {
    // Roughly 20 seconds a card. Deliberately approximate and shown as such —
    // a precise-looking estimate that's wrong is worse than a rough one.
    const mins = Math.max(1, Math.round((due * 20) / 60));
    return {
      eyebrow: "Due today",
      title: `${due} ${due === 1 ? "card is" : "cards are"} ready to review`,
      detail: `About ${mins} minute${mins === 1 ? "" : "s"}. Reviews get harder the longer they wait.`,
      action: "Start reviewing",
      route: "review",
      icon: <Layers size={16} />,
    };
  }

  const urgent = assessments.filter((a) => a.daysAway <= 3)[0];
  if (urgent) {
    return {
      eyebrow: urgent.daysAway === 0 ? "Today" : `In ${urgent.daysAway} days`,
      title: urgent.name,
      detail: `${urgent.subjectName}. Retain will show you what's worth revising for it.`,
      action: "See what to revise",
      route: "assessments",
      icon: <Layers size={16} />,
    };
  }

  const newCards = counts?.newRemainingTotal ?? 0;
  if (newCards > 0 && activeMinutes === 0) {
    return {
      eyebrow: "Ready when you are",
      title: `${newCards} new ${newCards === 1 ? "card" : "cards"} to learn`,
      detail: "Nothing is overdue. This is a good moment to get ahead.",
      action: "Learn something new",
      route: "review",
      icon: <Layers size={16} />,
    };
  }

  return {
    eyebrow: activeMinutes > 0 ? "Keep going" : "Ready when you are",
    title: activeMinutes > 0 ? "Start another session" : "Start a session",
    detail:
      activeMinutes > 0
        ? "Nothing is due. Pick a subject and work on whatever you choose."
        : "Pick a subject and the clock starts. It keeps counting if you close the window.",
    action: "Choose a subject",
    route: "timer",
    icon: <Play size={16} />,
  };
}

/** The greeting's second line. Never mentions what's been missed or lost. */
function openingLine(
  minutes: number,
  qualified: boolean,
  threshold: number,
): string {
  if (qualified && minutes >= 60) {
    const h = Math.floor(minutes / 60);
    return `${h} ${h === 1 ? "hour" : "hours"} in today. Anything more is a bonus.`;
  }
  if (qualified) return "Today's in. Anything more is a bonus.";
  if (minutes > 0) {
    return `${minutes} minutes down — ${Math.max(0, threshold - minutes)} more earns the day.`;
  }
  return "Ready to make a start?";
}

function DayRing({
  qualified,
  progress,
  minutes,
}: {
  qualified: boolean;
  progress: number;
  minutes: number;
}) {
  const size = 108;
  const stroke = 8;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const colour = qualified ? "var(--color-positive)" : "var(--accent)";

  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90" aria-hidden>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          stroke={colour}
          opacity={0.13}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          stroke={colour}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - (qualified ? 1 : progress))}
          style={{ transition: "stroke-dashoffset 700ms var(--ease)" }}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <div
          className="tabular text-[22px] font-medium leading-none tracking-[-0.02em]"
          style={{ color: qualified ? "var(--color-positive)" : "var(--ink)" }}
        >
          {minutes}
          <span className="text-[13px] font-normal text-[var(--ink-faint)]">
            m
          </span>
        </div>
        <div className="mt-1 text-[10.5px] text-[var(--ink-faint)]">
          {qualified ? "day earned" : "today"}
        </div>
      </div>
      <span className="sr-only">
        {qualified
          ? "Today has been earned."
          : `${Math.round(progress * 100)} percent of today's goal.`}
      </span>
    </div>
  );
}

/** A quiet supporting figure. Clickable ones get a hover state. */
function Figure({
  value,
  label,
  accent,
  icon,
  onClick,
}: {
  value: React.ReactNode;
  label: string;
  accent?: string;
  icon?: React.ReactNode;
  onClick?: () => void;
}) {
  const body = (
    <>
      <div
        className="tabular flex items-center gap-1.5 text-[19px] font-medium leading-none tracking-[-0.02em]"
        style={accent ? { color: accent } : undefined}
      >
        {icon}
        {value}
      </div>
      <div className="mt-1.5 text-[12px] text-[var(--ink-faint)]">{label}</div>
    </>
  );

  if (!onClick) return <div className="min-w-0">{body}</div>;

  return (
    <button
      onClick={onClick}
      className={cx(
        "pressable min-w-0 rounded-[var(--r-md)] px-2 py-1 text-left -mx-2",
        "transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]",
      )}
    >
      {body}
    </button>
  );
}

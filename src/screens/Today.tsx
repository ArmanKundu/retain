// The Today screen.
//
// Composed rather than grid-generated. There is exactly one hero — today's
// focused time — and everything else is deliberately quieter than it. A row of
// six equal cards gives every number the same weight, which means the screen
// tells you nothing at a glance; this one answers "how am I doing today?"
// before you've read a word.
//
// All data comes from the existing store (`streak`, `rings`, `recent`) and the
// existing Tauri commands. Nothing here computes its own statistics.

import { Flame, Play, Shield } from "lucide-react";

import { GoalRing } from "../components/GoalRing";
import { UpcomingEvents } from "../components/UpcomingEvents";
import { Metric, SectionHeader, SubjectPill } from "../components/primitives";
import { Button, Card, Empty } from "../components/ui";
import { duration, greeting, timeOfDay } from "../lib/format";
import { useApp } from "../store";

export function Today() {
  const { boot, streak, rings, recent, setRoute, timer } = useApp();

  const activeMinutes = streak?.todayActiveMinutes ?? 0;
  const threshold = streak?.thresholdMinutes ?? 20;
  // Clamped: the ring is a measure of the day being earned, not a score to beat.
  const dayProgress = Math.min(1, threshold > 0 ? activeMinutes / threshold : 0);

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 pb-16 sm:px-9">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-7">
        <h1 className="text-[28px] font-semibold tracking-[-0.028em]">
          {greeting(boot?.userName ?? "")}
        </h1>
        <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
          {todayLine(streak)}
        </p>
      </header>

      {/* Hero.
          The only place in the app with an atmospheric glow behind it. Its job
          is to make one number feel like the answer to the question you opened
          the app with, rather than one tile among several. */}
      <section className="animate-rise relative mb-6 overflow-hidden rounded-[var(--r-xl)] border border-[var(--line-soft)] bg-[color-mix(in_srgb,var(--surface)_82%,transparent)]">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0"
          style={{
            background:
              "radial-gradient(72% 120% at 8% 0%, color-mix(in srgb, var(--accent) 14%, transparent) 0%, transparent 62%), radial-gradient(56% 100% at 96% 100%, color-mix(in srgb, var(--color-positive) 11%, transparent) 0%, transparent 60%)",
          }}
        />

        <div className="relative p-7">
          <div className="flex items-center gap-6">
            <div className="min-w-0 flex-1">
              <Metric
                size="hero"
                value={activeMinutes >= 60 ? duration(activeMinutes * 60) : `${activeMinutes}m`}
                label="focused today"
              />
            </div>

            {streak && (
              <GoalRingLike qualified={streak.todayQualified} progress={dayProgress} />
            )}
          </div>

          {streak && (
            <div className="mt-6 flex flex-wrap items-center gap-x-8 gap-y-4 border-t border-[var(--line-soft)] pt-5">
              <Metric
                size="sm"
                value={streak.current}
                label={streak.current === 1 ? "day running" : "days running"}
                accent={streak.current > 0 ? "var(--warn)" : undefined}
                icon={
                  streak.current > 0 ? (
                    <Flame
                      size={17}
                      strokeWidth={1.9}
                      className="text-[var(--warn)]"
                      style={{
                        filter:
                          "drop-shadow(0 0 9px color-mix(in srgb, var(--warn) 45%, transparent))",
                      }}
                    />
                  ) : undefined
                }
              />

              <Metric size="sm" value={streak.longest} label="best run" />

              {streak.freezesAvailable > 0 && (
                <Metric
                  size="sm"
                  value={streak.freezesAvailable}
                  label={streak.freezesAvailable === 1 ? "freeze ready" : "freezes ready"}
                  icon={<Shield size={15} strokeWidth={1.9} className="text-[var(--ink-faint)]" />}
                />
              )}
            </div>
          )}
        </div>
      </section>

      {!timer && (
        <Button
          size="lg"
          variant="primary"
          className="animate-rise mb-8"
          onClick={() => setRoute("timer")}
        >
          <Play size={16} />
          Start a session
        </Button>
      )}

      {rings.length > 0 && (
        <section className="animate-rise mb-8">
          <SectionHeader title="This week" hint="hours against your goals" />
          <div className="flex flex-wrap gap-x-8 gap-y-6 px-1">
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
          <Card>
            <Empty
              title="Nothing logged yet"
              body="Sessions show up here once you've finished one, with what you noted at the time."
            />
          </Card>
        ) : (
          // A list, not a stack of cards. Rows separated by hairlines and given
          // a hover state read as one object with parts; boxed rows read as
          // several unrelated objects.
          <div className="-mx-3">
            {recent.map((s) => (
              <div
                key={s.id}
                className="flex items-start gap-3 rounded-[var(--r-md)] px-3 py-3 transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
              >
                <div className="mt-[5px]">
                  <SubjectPill name={s.subjectName} colour={s.colour} dotOnly />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-2">
                    <span className="text-[13.5px] text-[var(--ink)]">{s.subjectName}</span>
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
                      {s.idlePauseCount > 0 && ` · ${s.idlePauseCount} from inactivity`}
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

/**
 * The day's progress toward being earned.
 *
 * A ring rather than a bar, and it fills to completion and stops — the day is
 * a threshold that's met, not a score. Once met it goes green and says so.
 */
function GoalRingLike({ qualified, progress }: { qualified: boolean; progress: number }) {
  const size = 62;
  const stroke = 6;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const colour = qualified ? "var(--color-positive)" : "var(--accent)";

  return (
    <div className="relative" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90" aria-hidden>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" strokeWidth={stroke} stroke={colour} opacity={0.14} />
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
      <div
        className="absolute inset-0 flex items-center justify-center text-[11px] font-medium"
        style={{ color: qualified ? "var(--color-positive)" : "var(--ink-dim)" }}
      >
        {qualified ? "done" : `${Math.round(progress * 100)}%`}
      </div>
      <span className="sr-only">
        {qualified ? "Today has been earned." : `${Math.round(progress * 100)} percent of today's goal.`}
      </span>
    </div>
  );
}

/**
 * The subtitle under the greeting.
 *
 * Every branch here is forward-looking. There is no version of this that
 * mentions a broken streak, a missed day, or what's about to be lost — the brief
 * is explicit that framing stays on progress, and this is the line most likely to
 * drift into guilt if left unattended.
 */
function todayLine(streak: ReturnType<typeof useApp.getState>["streak"]): string {
  if (!streak) return "Ready when you are.";
  if (streak.todayQualified) return "Today's in. Anything more is a bonus.";
  if (streak.todayActiveMinutes > 0)
    return `${streak.todayActiveMinutes} minutes down. ${Math.max(
      0,
      streak.thresholdMinutes - streak.todayActiveMinutes,
    )} more earns the day.`;
  return `One ${streak.thresholdMinutes}-minute session earns today.`;
}

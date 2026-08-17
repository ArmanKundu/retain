import { useState } from "react";
import { Flame, Shield } from "lucide-react";

import { ContributionGrid } from "../components/ContributionGrid";
import { DayDetailSheet } from "../components/DayDetailSheet";
import { GoalRing } from "../components/GoalRing";
import { WeeklyReview } from "../components/WeeklyReview";
import { Card, Empty, SectionTitle } from "../components/ui";
import { WEEKDAY_LABELS, duration } from "../lib/format";
import { useApp } from "../store";

export function Progress() {
  const [openDay, setOpenDay] = useState<string | null>(null);
  const { grid, streak, rings, setRoute } = useApp();

  const totalMinutes = grid.reduce((sum, d) => sum + d.minutes, 0);
  const activeDays = grid.filter((d) => d.minutes > 0).length;

  return (
    <div className="mx-auto max-w-[880px] px-9 pb-14">
      {/* Content scrolls under the title bar. macOS separates the two with a
          hard edge rather than letting text vanish mid-letter. */}
      <div className="titlebar-drag scroll-edge h-11" />

      <header className="animate-in mb-7">
        <h1 className="text-[28px] font-semibold tracking-[var(--track-display)]">
          Progress
        </h1>
        <p className="mt-1 text-[14px] text-[var(--ink-dim)]">
          {activeDays > 0
            ? `${duration(totalMinutes * 60)} across ${activeDays} ${activeDays === 1 ? "day" : "days"}.`
            : "Your first session will show up here."}
        </p>
      </header>

      <Card className="animate-in mb-5 p-5">
        <ContributionGrid days={grid} onSelect={setOpenDay} />
      </Card>

      {streak && (
        <div className="animate-in mb-5 grid grid-cols-3 gap-3">
          <Stat
            icon={<Flame size={15} className="text-[var(--warn)]" />}
            value={streak.current}
            label={streak.current === 1 ? "day running" : "days running"}
          />
          <Stat value={streak.longest} label="best run" />
          <Stat
            icon={<Shield size={14} className="text-[var(--ink-faint)]" />}
            value={streak.freezesAvailable}
            label={
              streak.freezesAvailable === 1 ? "freeze ready" : "freezes ready"
            }
          />
        </div>
      )}

      {streak && (
        <Card className="animate-in mb-5 p-5">
          <SectionTitle>How a day is earned</SectionTitle>
          <ul className="mt-3 space-y-2 text-[13px] leading-relaxed text-[var(--ink-dim)]">
            <li>
              One session with at least{" "}
              <span className="text-[var(--ink)]">
                {streak.thresholdMinutes} minutes
              </span>{" "}
              of active time — pauses, idle time and breaks don't count toward
              it.
            </li>
            <li>Or clearing every review that was due that day.</li>
            {streak.restDays.length > 0 && (
              <li>
                {streak.restDays.map((d) => WEEKDAY_LABELS[d]).join(" and ")}{" "}
                {streak.restDays.length === 1
                  ? "is a rest day"
                  : "are rest days"}{" "}
                — they never break a run.
              </li>
            )}
            <li className="text-[var(--ink-faint)]">
              A freeze covers a missed day on its own. You get one back every 7
              earned days, up to two.
            </li>
          </ul>
        </Card>
      )}

      <section className="animate-in">
        <SectionTitle>Weekly goals</SectionTitle>
        <Card className="mt-2.5 p-5">
          {rings.length === 0 ? (
            <Empty
              title="No weekly goals set"
              body="Give a subject an hours-per-week target in Settings and it'll show up here as a ring."
            />
          ) : (
            <div className="flex flex-wrap justify-around gap-6">
              {rings.map((r) => (
                <GoalRing key={r.subjectId} ring={r} size={84} />
              ))}
            </div>
          )}
        </Card>
      </section>

      <WeeklyReview onOpenSettings={() => setRoute("settings")} />

      {openDay && (
        <DayDetailSheet date={openDay} onClose={() => setOpenDay(null)} />
      )}
    </div>
  );
}

function Stat({
  icon,
  value,
  label,
}: {
  icon?: React.ReactNode;
  value: number;
  label: string;
}) {
  return (
    <Card className="p-4">
      <div className="flex items-center gap-1.5">
        {icon}
        <span className="tabular text-[24px] font-semibold leading-none tracking-[var(--track-display)]">
          {value}
        </span>
      </div>
      <div className="mt-1.5 text-[12px] text-[var(--ink-faint)]">{label}</div>
    </Card>
  );
}

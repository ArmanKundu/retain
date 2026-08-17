import { useMemo, useState } from "react";

import { cx } from "./ui";

import { addDays, duration, localDate, prettyDate } from "../lib/format";
import type { GridDay } from "../lib/types";
import { ColourDot } from "./ui";

const CELL = 11;
const GAP = 3;
const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/**
 * A year of study, GitHub-style.
 *
 * One deliberate difference from GitHub: cells are tinted with the colour of the
 * subject you spent the most time on that day, rather than a single hue. It costs
 * nothing and turns the grid from "how much" into "how much, and on what" — you
 * can see a fortnight of nothing but Biology at a glance.
 */
export function ContributionGrid({
  days,
  onSelect,
}: {
  days: GridDay[];
  /** Clicking a day opens its breakdown. Optional — the grid still works alone. */
  onSelect?: (date: string) => void;
}) {
  const [hovered, setHovered] = useState<{
    day: GridDay;
    x: number;
    y: number;
  } | null>(null);

  const byDate = useMemo(() => new Map(days.map((d) => [d.date, d])), [days]);

  // Columns are weeks. Start on the Monday on or before 52 weeks ago so every
  // column is a full week and rows line up with weekdays.
  const { weeks, monthLabels } = useMemo(() => {
    const today = new Date();
    const start = addDays(today, -364);
    start.setDate(start.getDate() - ((start.getDay() + 6) % 7));

    const weeks: string[][] = [];
    const monthLabels: { index: number; label: string }[] = [];
    let cursor = new Date(start);
    let lastMonth = -1;

    while (cursor <= today) {
      const week: string[] = [];
      for (let d = 0; d < 7; d++) {
        week.push(localDate(cursor));
        cursor = addDays(cursor, 1);
      }
      // Label a column when its first day starts a new month.
      const firstOfWeek = new Date(week[0]);
      if (firstOfWeek.getMonth() !== lastMonth) {
        lastMonth = firstOfWeek.getMonth();
        monthLabels.push({ index: weeks.length, label: MONTHS[lastMonth] });
      }
      weeks.push(week);
    }

    return { weeks, monthLabels };
  }, []);

  const today = localDate();

  return (
    <div className="relative">
      <div className="overflow-x-auto pb-1">
        <div style={{ minWidth: weeks.length * (CELL + GAP) + 30 }}>
          {/* Month labels */}
          <div className="relative mb-1.5 ml-[30px] h-3">
            {monthLabels.map((m) => (
              <span
                key={`${m.label}-${m.index}`}
                className="absolute text-[10px] text-[var(--ink-faint)]"
                style={{ left: m.index * (CELL + GAP) }}
              >
                {m.label}
              </span>
            ))}
          </div>

          <div className="flex gap-[3px]">
            {/* Weekday labels — only alternate rows, as GitHub does, to keep it quiet */}
            <div className="mr-1 flex w-[26px] flex-col gap-[3px]">
              {["", "Tue", "", "Thu", "", "Sat", ""].map((label, i) => (
                <span
                  key={i}
                  className="text-[9.5px] leading-none text-[var(--ink-faint)]"
                  style={{ height: CELL, lineHeight: `${CELL}px` }}
                >
                  {label}
                </span>
              ))}
            </div>

            {weeks.map((week, wi) => (
              <div key={wi} className="flex flex-col gap-[3px]">
                {week.map((date) => {
                  const day = byDate.get(date);
                  const future = date > today;
                  const clickable = !!day && !future && !!onSelect;
                  return (
                    <div
                      key={date}
                      role={clickable ? "button" : undefined}
                      tabIndex={clickable ? 0 : undefined}
                      aria-label={clickable ? `See ${date}` : undefined}
                      onClick={() => clickable && onSelect(date)}
                      onKeyDown={(e) => {
                        if (clickable && (e.key === "Enter" || e.key === " ")) {
                          e.preventDefault();
                          onSelect(date);
                        }
                      }}
                      onMouseEnter={(e) => {
                        if (!day) return;
                        const r = e.currentTarget.getBoundingClientRect();
                        setHovered({ day, x: r.left + r.width / 2, y: r.top });
                      }}
                      onMouseLeave={() => setHovered(null)}
                      className={cx(
                        "rounded-[2.5px] transition-transform duration-100 hover:scale-[1.35]",
                        clickable && "cursor-pointer",
                      )}
                      style={{
                        width: CELL,
                        height: CELL,
                        background: future ? "transparent" : cellColour(day),
                        outline: day?.qualified
                          ? "1px solid rgba(255,255,255,0.14)"
                          : undefined,
                        outlineOffset: -1,
                      }}
                    />
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Legend */}
      <div className="mt-3 flex items-center gap-1.5 text-[10.5px] text-[var(--ink-faint)]">
        <span>Less</span>
        {[0, 20, 45, 90, 180].map((m) => (
          <span
            key={m}
            className="rounded-[2.5px]"
            style={{
              width: CELL,
              height: CELL,
              background: cellColour(
                m === 0
                  ? undefined
                  : ({ minutes: m, bySubject: [] } as unknown as GridDay),
              ),
            }}
          />
        ))}
        <span>More</span>
      </div>

      {hovered && <Tooltip day={hovered.day} x={hovered.x} y={hovered.y} />}
    </div>
  );
}

/**
 * Empty days get a faint surface tint. Days with study get the dominant
 * subject's colour at an opacity stepped by minutes — four steps, because more
 * than that stops being distinguishable at 11 pixels.
 */
function cellColour(day?: GridDay): string {
  if (!day || day.minutes === 0)
    return "color-mix(in srgb, var(--ink-faint) 13%, transparent)";

  const alpha =
    day.minutes >= 180
      ? 1
      : day.minutes >= 90
        ? 0.78
        : day.minutes >= 45
          ? 0.55
          : 0.33;
  const base = day.bySubject[0]?.colour ?? "var(--accent)";
  return `color-mix(in srgb, ${base} ${alpha * 100}%, transparent)`;
}

function Tooltip({ day, x, y }: { day: GridDay; x: number; y: number }) {
  return (
    <div
      className="glass pointer-events-none fixed z-40 -translate-x-1/2 -translate-y-full rounded-[var(--r-md)] px-3 py-2.5"
      style={{ left: x, top: y - 8 }}
    >
      <div className="text-[12px] font-medium text-[var(--ink)]">
        {prettyDate(day.date)}
      </div>
      <div className="mt-0.5 text-[11.5px] text-[var(--ink-dim)]">
        {day.minutes === 0 ? "No sessions" : duration(day.minutes * 60)}
      </div>

      {day.bySubject.length > 0 && (
        <div className="mt-2 space-y-1 border-t border-[var(--line-soft)] pt-2">
          {day.bySubject.map((s) => (
            <div
              key={s.subjectId}
              className="flex items-center gap-2 whitespace-nowrap"
            >
              <ColourDot colour={s.colour} size={7} />
              <span className="text-[11.5px] text-[var(--ink-dim)]">
                {s.subjectName}
              </span>
              <span className="tabular ml-auto pl-3 text-[11.5px] text-[var(--ink-faint)]">
                {duration(s.minutes * 60)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

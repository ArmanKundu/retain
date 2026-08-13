// The next few days of your school calendar, on the Today screen.
//
// Grouped by day rather than listed flat. The flat version repeated the weekday
// on every row — six rows all saying "Friday" — which is noise standing in for
// structure. A day heading says it once, and the rows underneath become a
// timetable you can read down.
//
// Deliberately not a calendar application: no grid, no month view, no
// navigation. It answers "what's on, and when do I have gaps" and stops.

import { useEffect, useMemo, useState } from "react";

import { api } from "../lib/api";
import type { CalendarEvent } from "../lib/types";
import { SectionHeader } from "./primitives";
import { cx } from "./ui";

/** Local clock time, compact: `8:30am`, `1:55pm`. */
function clockOf(iso: string): string {
  return new Date(iso)
    .toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    .replace(/\s/g, "")
    .toLowerCase();
}

function isoDay(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

/** "Today", "Tomorrow", then the weekday, then a date once it's far enough out. */
function dayLabel(iso: string): { primary: string; secondary: string } {
  const today = new Date();
  const target = new Date(`${iso}T12:00:00`);
  const days = Math.round(
    (target.getTime() - new Date(`${isoDay(today)}T12:00:00`).getTime()) / 86_400_000,
  );

  const weekday = target.toLocaleDateString(undefined, { weekday: "long" });
  const date = target.toLocaleDateString(undefined, { day: "numeric", month: "short" });

  if (days === 0) return { primary: "Today", secondary: date };
  if (days === 1) return { primary: "Tomorrow", secondary: date };
  if (days < 7) return { primary: weekday, secondary: date };
  return { primary: weekday, secondary: date };
}

export function UpcomingEvents({ days = 7, limit = 40 }: { days?: number; limit?: number }) {
  const [events, setEvents] = useState<CalendarEvent[]>([]);

  useEffect(() => {
    void api
      .upcomingEvents(days, limit)
      // A calendar problem must never take out the Today screen.
      .then(setEvents)
      .catch(() => setEvents([]));
  }, [days, limit]);

  // Grouped in render order. `upcoming` already returns them sorted by start,
  // so insertion order into the map is chronological.
  const grouped = useMemo(() => {
    const map = new Map<string, CalendarEvent[]>();
    for (const e of events) {
      const bucket = map.get(e.localDate);
      if (bucket) bucket.push(e);
      else map.set(e.localDate, [e]);
    }
    return [...map.entries()];
  }, [events]);

  if (grouped.length === 0) return null;

  const today = isoDay(new Date());

  return (
    <section className="animate-rise mb-8">
      <SectionHeader title="On your calendar" hint={`next ${days} days`} />

      <div className="space-y-5">
        {grouped.map(([day, items]) => {
          const { primary, secondary } = dayLabel(day);
          const isToday = day === today;

          return (
            <div key={day}>
              <div className="mb-1.5 flex items-baseline gap-2 px-1">
                <span
                  className={cx(
                    "text-[12.5px] font-medium",
                    isToday ? "text-[var(--accent)]" : "text-[var(--ink-dim)]",
                  )}
                >
                  {primary}
                </span>
                <span className="text-[11.5px] text-[var(--ink-faint)]">{secondary}</span>
                <span className="ml-auto text-[11.5px] text-[var(--ink-faint)]">
                  {items.length} {items.length === 1 ? "class" : "classes"}
                </span>
              </div>

              {/* A hairline rail down the left ties the day's rows together
                  without boxing each one. */}
              <div className="relative border-l border-[var(--line-soft)] pl-3">
                {items.map((e) => (
                  <div
                    key={`${e.uid}-${e.recurrenceId ?? ""}`}
                    className="group flex items-baseline gap-3 rounded-[var(--r-sm)] px-2 py-[7px] transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
                  >
                    {/* Time first and monospaced, so the column aligns and the
                        day reads as a schedule rather than a list of names. */}
                    <span className="tabular w-[62px] shrink-0 text-[12.5px] text-[var(--ink-faint)]">
                      {e.allDay ? "all day" : clockOf(e.startsAt)}
                    </span>

                    <span className="min-w-0 flex-1 truncate text-[13.5px] text-[var(--ink)]">
                      {e.summary}
                    </span>

                    {e.endsAt && !e.allDay && (
                      <span className="tabular shrink-0 text-[11.5px] text-[var(--ink-faint)] opacity-0 transition-opacity duration-[var(--t-fast)] group-hover:opacity-100">
                        until {clockOf(e.endsAt)}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

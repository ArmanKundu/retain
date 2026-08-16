// Today's timetable.
//
// This replaced a list of seven days of raw class codes — `11ENGT2`, `11ACCQ`,
// `12BIOS`, one under the other with a time beside each and nothing else. It
// read as a database dump because that is what it was: Compass sends a class as
// three separate ICS properties and Retain was showing one of them.
//
// What it shows now is what a timetable is for at 8:25 in the morning: what's
// next, where it is, who's taking it. The subject's own colour runs down the
// left so the day is scannable without reading a word, and the class happening
// right now is marked, because "which period am I in" is a question you ask
// several times a day.
//
// Deliberately one day, not seven. A week of classes is a wall, and the week
// grid already exists for when you want it.

import { useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, MapPin, User } from "lucide-react";

import { api } from "../lib/api";
import type { ScheduledClass } from "../lib/types";
import { cx } from "./ui";
import { SectionHeader } from "./primitives";

function isoOf(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

function clockOf(iso: string): string {
  const d = new Date(iso);
  const h = d.getHours();
  const m = d.getMinutes();
  const suffix = h < 12 ? "am" : "pm";
  const display = h % 12 === 0 ? 12 : h % 12;
  return m === 0
    ? `${display}${suffix}`
    : `${display}:${String(m).padStart(2, "0")}${suffix}`;
}

export function DaySchedule() {
  const [offset, setOffset] = useState(0);
  const [classes, setClasses] = useState<ScheduledClass[]>([]);
  const [loaded, setLoaded] = useState(false);
  // Re-rendered each minute so "now" stays true without a full reload.
  const [tick, setTick] = useState(() => Date.now());

  const date = useMemo(() => {
    const d = new Date();
    d.setDate(d.getDate() + offset);
    return d;
  }, [offset]);

  useEffect(() => {
    setLoaded(false);
    void api
      .daySchedule(isoOf(date))
      .then(setClasses)
      .catch(() => setClasses([]))
      .finally(() => setLoaded(true));
  }, [date]);

  useEffect(() => {
    const t = setInterval(() => setTick(Date.now()), 60_000);
    return () => clearInterval(t);
  }, []);

  const timed = classes.filter((c) => !c.allDay);
  const allDay = classes.filter((c) => c.allDay);

  // Which class is happening right now — only meaningful on today itself.
  const nowIndex =
    offset === 0
      ? timed.findIndex((c) => {
          const start = new Date(c.startsAt).getTime();
          const end = c.endsAt
            ? new Date(c.endsAt).getTime()
            : start + 45 * 60_000;
          return tick >= start && tick < end;
        })
      : -1;

  const label =
    offset === 0
      ? "Today"
      : offset === 1
        ? "Tomorrow"
        : date.toLocaleDateString(undefined, { weekday: "long" });

  return (
    <section className="animate-rise mb-9">
      <SectionHeader
        title={label}
        hint={date.toLocaleDateString(undefined, {
          day: "numeric",
          month: "long",
        })}
      >
        <button
          onClick={() => setOffset((o) => o - 1)}
          aria-label="Previous day"
          className="pressable rounded-[var(--r-sm)] border border-[var(--line)] p-1 text-[var(--ink-dim)] hover:text-[var(--ink)]"
        >
          <ChevronLeft size={14} />
        </button>
        {offset !== 0 && (
          <button
            onClick={() => setOffset(0)}
            className="pressable rounded-[var(--r-sm)] border border-[var(--line)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
          >
            Today
          </button>
        )}
        <button
          onClick={() => setOffset((o) => o + 1)}
          aria-label="Next day"
          className="pressable rounded-[var(--r-sm)] border border-[var(--line)] p-1 text-[var(--ink-dim)] hover:text-[var(--ink)]"
        >
          <ChevronRight size={14} />
        </button>
      </SectionHeader>

      {/* Whole-day things — excursions, ensembles — sit above the periods
          rather than being given a fake time slot. */}
      {allDay.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-1.5">
          {allDay.map((c, i) => (
            <span
              key={`${c.code}-${i}`}
              className="rounded-full border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)]"
            >
              {c.code}
            </span>
          ))}
        </div>
      )}

      {!loaded ? null : timed.length === 0 ? (
        <p className="px-1 text-[13.5px] leading-relaxed text-[var(--ink-dim)]">
          {allDay.length > 0
            ? "No timetabled classes."
            : "Nothing on. The day is yours."}
        </p>
      ) : (
        <ul className="space-y-1">
          {timed.map((c, i) => (
            <ClassRow
              key={`${c.code}-${c.startsAt}`}
              item={c}
              now={i === nowIndex}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function ClassRow({ item, now }: { item: ScheduledClass; now: boolean }) {
  // A class that isn't one of your subjects — an assembly, a formal — has no
  // colour, and a neutral line is more honest than borrowing another
  // subject's.
  const tint = item.colour ?? "var(--ink-faint)";

  return (
    <li
      className={cx(
        "flex items-center gap-3 rounded-[var(--r-md)] border py-2.5 pl-0 pr-3.5 transition-colors duration-[var(--t-fast)]",
        now
          ? "border-[var(--accent)]/35 bg-[var(--accent)]/8"
          : "border-transparent hover:border-[var(--line-soft)] hover:bg-[var(--surface)]",
      )}
    >
      {/* The subject's colour, so the shape of the day is readable at a
          glance without parsing any text. */}
      <span
        aria-hidden
        className="ml-1 h-8 w-[3px] shrink-0 rounded-full"
        style={{ background: tint }}
      />

      <div className="w-[62px] shrink-0 text-right text-[12.5px] tabular-nums text-[var(--ink-faint)]">
        {clockOf(item.startsAt)}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-[14px] text-[var(--ink)]">
            {item.subjectName ?? item.code}
          </span>
          {/* The code stays visible when it isn't the headline, because it's
              what's printed on the timetable you're comparing against. */}
          {item.subjectName && (
            <span className="shrink-0 text-[11.5px] text-[var(--ink-faint)]">
              {item.code}
            </span>
          )}
          {now && (
            <span className="shrink-0 rounded-full bg-[var(--accent)]/18 px-2 py-0.5 text-[11px] text-[var(--accent)]">
              now
            </span>
          )}
        </div>

        {(item.room || item.teacher) && (
          <div className="mt-0.5 flex items-center gap-3 text-[11.5px] text-[var(--ink-faint)]">
            {item.room && (
              <span className="flex items-center gap-1">
                <MapPin size={10} />
                {item.room}
              </span>
            )}
            {item.teacher && (
              <span className="flex items-center gap-1">
                <User size={10} />
                {item.teacher}
              </span>
            )}
          </div>
        )}
      </div>
    </li>
  );
}

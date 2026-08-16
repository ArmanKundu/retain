// What's coming, and how long you've got.
//
// The Today screen listed seven days of classes and called that "your
// calendar", which answered a question nobody asks — you know you have English
// on Thursday. The question worth answering is the other one: what's coming
// that you have to *do something about*, and how much time is left.
//
// So this is assessments and one-off events only. Recurring classes are
// deliberately excluded: a SAC in eleven days is news, Tuesday's Chemistry
// period is not, and mixing them buries the first under forty of the second.

import { useEffect, useState } from "react";
import { CalendarClock } from "lucide-react";

import { api } from "../lib/api";
import { SectionHeader } from "./primitives";
import { cx } from "./ui";

/** How far out to look. Beyond a fortnight nothing is actionable yet. */
const HORIZON_DAYS = 21;

interface Ahead {
  key: string;
  title: string;
  when: Date;
  daysAway: number;
  /** Assessments are yours; events come from the school feed. */
  kind: "assessment" | "event";
  subject: string | null;
  colour: string | null;
}

function daysBetween(from: Date, to: Date): number {
  const a = new Date(
    from.getFullYear(),
    from.getMonth(),
    from.getDate(),
  ).getTime();
  const b = new Date(to.getFullYear(), to.getMonth(), to.getDate()).getTime();
  return Math.round((b - a) / 86400000);
}

/**
 * Whether an event is a recurring class rather than something notable.
 *
 * Compass class codes are a year prefix and a subject stem — `11CHEU2`,
 * `12BIOS` — and always start with digits. Anything with a real name ("Year 11
 * Formal", "Division Athletics") does not, which turns out to be a reliable
 * separator on a Compass feed and costs nothing when it's wrong: the worst case
 * is one extra row.
 */
function isClassCode(summary: string): boolean {
  return /^\d/.test(summary.trim());
}

export function LookingAhead() {
  const [items, setItems] = useState<Ahead[]>([]);

  useEffect(() => {
    void (async () => {
      const today = new Date();
      const out: Ahead[] = [];

      try {
        for (const a of await api.listAssessments(false)) {
          if (a.daysAway < 0 || a.daysAway > HORIZON_DAYS) continue;
          out.push({
            key: `a${a.id}`,
            title: a.name,
            when: new Date(`${a.dueOn}T12:00:00`),
            daysAway: a.daysAway,
            kind: "assessment",
            subject: a.subjectName,
            colour: a.colour,
          });
        }
      } catch {
        // An assessment failure costs those rows, not the section.
      }

      try {
        for (const e of await api.upcomingEvents(HORIZON_DAYS, 200)) {
          if (isClassCode(e.summary)) continue;
          const when = new Date(e.startsAt);
          const away = daysBetween(today, when);
          if (away < 0 || away > HORIZON_DAYS) continue;
          out.push({
            key: `e${e.uid}${e.startsAt}`,
            title: e.summary,
            when,
            daysAway: away,
            kind: "event",
            subject: null,
            colour: null,
          });
        }
      } catch {
        setItems(out.sort((x, y) => x.when.getTime() - y.when.getTime()));
        return;
      }

      setItems(out.sort((x, y) => x.when.getTime() - y.when.getTime()));
    })();
  }, []);

  if (items.length === 0) return null;

  return (
    <section className="animate-rise mb-9">
      <SectionHeader title="Looking ahead" hint={`next ${HORIZON_DAYS} days`} />

      <ul className="space-y-1">
        {items.map((item) => (
          <li
            key={item.key}
            className="flex items-center gap-3 rounded-[var(--r-md)] px-1 py-2.5 hover:bg-[var(--surface)]"
          >
            <span
              aria-hidden
              className="h-8 w-[3px] shrink-0 rounded-full"
              style={{ background: item.colour ?? "var(--ink-faint)" }}
            />

            <div className="min-w-0 flex-1">
              <div className="flex items-baseline gap-2">
                <span className="truncate text-[14px] text-[var(--ink)]">
                  {item.title}
                </span>
                {item.subject && (
                  <span className="shrink-0 text-[11.5px] text-[var(--ink-faint)]">
                    {item.subject}
                  </span>
                )}
              </div>
              <div className="mt-0.5 flex items-center gap-1.5 text-[11.5px] text-[var(--ink-faint)]">
                <CalendarClock size={10} />
                {item.when.toLocaleDateString(undefined, {
                  weekday: "short",
                  day: "numeric",
                  month: "short",
                })}
              </div>
            </div>

            {/* The number that actually changes behaviour. Assessments inside a
                week are warm, inside three days urgent — the school's own
                events never are, because you don't revise for a formal. */}
            <span
              className={cx(
                "shrink-0 rounded-full px-2.5 py-1 text-[11.5px] tabular-nums",
                item.kind === "assessment" && item.daysAway <= 3
                  ? "bg-[var(--danger)]/15 text-[var(--danger)]"
                  : item.kind === "assessment" && item.daysAway <= 7
                    ? "bg-[var(--warn)]/15 text-[var(--warn)]"
                    : "text-[var(--ink-faint)]",
              )}
            >
              {item.daysAway === 0
                ? "today"
                : item.daysAway === 1
                  ? "tomorrow"
                  : `${item.daysAway} days`}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

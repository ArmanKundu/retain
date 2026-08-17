// One day, opened from the contribution grid.
//
// The grid answers "how much" and always prompted the follow-up it couldn't
// answer: *on what?* This is that answer — per-subject time, session counts,
// and whatever you noted at the time.
//
// Deliberately read-only. Editing history from a heatmap is how a study log
// stops being a record of what happened.

import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { api } from "../lib/api";
import { duration } from "../lib/format";
import type { DayDetail } from "../lib/types";

/** "Monday 17 August" — the way you'd say it, not an ISO string. */
function longDate(iso: string): string {
  return new Date(`${iso}T12:00:00`).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

export function DayDetailSheet({
  date,
  onClose,
}: {
  date: string;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<DayDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .dayDetail(date)
      .then(setDetail)
      .catch((e) => setError(String(e)));
  }, [date]);

  const busiest = detail?.bySubject[0]?.minutes ?? 0;

  return (
    <div
      className="scrim fixed inset-0 z-50 flex items-center justify-center px-8"
      onClick={onClose}
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <div
        className="sheet animate-pop w-full max-w-[480px] p-7"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h2 className="text-[20px] font-semibold tracking-[var(--track-display)]">
              {longDate(date)}
            </h2>
            {detail && (
              <p className="mt-1 text-[13px] text-[var(--ink-dim)]">
                {detail.totalMinutes === 0
                  ? "Nothing logged."
                  : `${duration(detail.totalMinutes * 60)} across ${detail.sessionCount} ${
                      detail.sessionCount === 1 ? "session" : "sessions"
                    }${detail.qualified ? " · day earned" : ""}`}
              </p>
            )}
          </div>
          <button
            onClick={onClose}
            aria-label="Close"
            className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:bg-[var(--surface-hi)] hover:text-[var(--ink)]"
          >
            <X size={15} />
          </button>
        </div>

        {error && (
          <p className="mt-4 text-[12.5px] text-[var(--danger)]">{error}</p>
        )}

        {detail && detail.bySubject.length > 0 && (
          <div className="mt-6 space-y-2.5">
            {detail.bySubject.map((s) => (
              <div key={s.subjectId} className="flex items-center gap-3">
                <span className="w-[124px] shrink-0 truncate text-[13px] text-[var(--ink-dim)]">
                  {s.subjectName}
                </span>
                {/* Bars are relative to the day's busiest subject, not to a
                    fixed scale — the comparison that matters here is between
                    subjects on this day. */}
                <div className="h-[7px] flex-1 overflow-hidden rounded-full bg-[var(--surface-hi)]">
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${busiest > 0 ? Math.max(4, (s.minutes / busiest) * 100) : 0}%`,
                      background: s.colour,
                    }}
                  />
                </div>
                <span className="tabular w-[56px] shrink-0 text-right text-[12.5px] text-[var(--ink-dim)]">
                  {duration(s.minutes * 60)}
                </span>
              </div>
            ))}
          </div>
        )}

        {detail && detail.notes.length > 0 && (
          <div className="mt-6 border-t border-[var(--line-soft)] pt-5">
            <div className="text-[12.5px] font-medium text-[var(--ink-dim)]">
              What you noted
            </div>
            <ul className="mt-2 space-y-1.5">
              {detail.notes.map((n, i) => (
                <li
                  key={i}
                  className="selectable text-[13px] leading-relaxed text-[var(--ink-dim)]"
                >
                  {n}
                </li>
              ))}
            </ul>
          </div>
        )}

        {detail && detail.bySubject.length === 0 && (
          <p className="mt-6 text-[13px] leading-relaxed text-[var(--ink-faint)]">
            A quiet day. Rest days and freezes exist so this doesn't have to
            mean anything.
          </p>
        )}
      </div>
    </div>
  );
}

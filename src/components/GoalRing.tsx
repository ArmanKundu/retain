import { hoursLabel } from "../lib/format";
import type { WeeklyGoalRing } from "../lib/types";

/**
 * An Apple-Watch-style progress ring.
 *
 * Drawn as an SVG arc rather than a bar because the ring reads as "a portion of
 * a whole" without needing a percentage label next to it, and it survives being
 * shrunk to 56px when there are six subjects in a row.
 *
 * Progress is capped at 1 for the arc — going past your goal shouldn't wrap the
 * ring around a second time and make 110% look like 10%.
 */
export function GoalRing({
  ring,
  size = 72,
}: {
  ring: WeeklyGoalRing;
  size?: number;
}) {
  const stroke = size / 9;
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;

  const ratio = ring.goalMinutes > 0 ? ring.doneMinutes / ring.goalMinutes : 0;
  const shown = Math.min(1, ratio);
  const complete = ratio >= 1;

  return (
    <div className="flex flex-col items-center gap-2">
      <div className="relative" style={{ width: size, height: size }}>
        <svg width={size} height={size} className="-rotate-90">
          {/* Track */}
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            strokeWidth={stroke}
            stroke={ring.colour}
            opacity={0.16}
          />
          {/* Progress */}
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            strokeWidth={stroke}
            stroke={ring.colour}
            strokeLinecap="round"
            strokeDasharray={circumference}
            strokeDashoffset={circumference * (1 - shown)}
            style={{
              transition: "stroke-dashoffset 600ms var(--ease-out-soft)",
            }}
          />
        </svg>

        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="tabular text-[13px] font-medium leading-none text-[var(--ink)]">
            {Math.round(ratio * 100)}
            <span className="text-[9px] text-[var(--ink-faint)]">%</span>
          </span>
          {complete && (
            <span className="mt-0.5 text-[8.5px] uppercase tracking-wide text-[var(--ink-faint)]">
              done
            </span>
          )}
        </div>
      </div>

      <div className="text-center">
        <div className="max-w-[86px] truncate text-[11.5px] text-[var(--ink-dim)]">
          {ring.subjectName}
        </div>
        <div className="tabular text-[10.5px] text-[var(--ink-faint)]">
          {hoursLabel(ring.doneMinutes)} / {hoursLabel(ring.goalMinutes)}
        </div>
      </div>
    </div>
  );
}

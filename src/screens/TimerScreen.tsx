import { useState } from "react";
import { Coffee, Pause, Play, Square } from "lucide-react";

import { api } from "../lib/api";
import { clock, duration } from "../lib/format";
import type { FinishedSession, TimerMode } from "../lib/types";
import { Button, Card, ColourDot, Segmented, cx } from "../components/ui";
import { useApp } from "../store";

const PRESETS = [
  { label: "25 / 5", work: 25, break: 5 },
  { label: "50 / 10", work: 50, break: 10 },
];

export function TimerScreen({
  onFinished,
}: {
  onFinished: (s: FinishedSession) => void;
}) {
  const { subjects, timer, setTimer, boot } = useApp();

  const [subjectId, setSubjectId] = useState<number | null>(subjects[0]?.id ?? null);
  const [mode, setMode] = useState<TimerMode>("stopwatch");
  const [work, setWork] = useState(boot?.pomodoroWorkMinutes ?? 25);
  const [brk, setBrk] = useState(boot?.pomodoroBreakMinutes ?? 5);
  const [error, setError] = useState<string | null>(null);

  const start = async () => {
    if (subjectId == null) return;
    setError(null);
    try {
      setTimer(
        await api.startTimer({
          subjectId,
          topicId: null,
          mode,
          workMinutes: mode === "pomodoro" ? work : null,
          breakMinutes: mode === "pomodoro" ? brk : null,
        }),
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const stop = async () => {
    const finished = await api.stopTimer();
    setTimer(null);
    if (finished) onFinished(finished);
  };

  // ---- Running ----------------------------------------------------------
  if (timer) {
    const paused = timer.pausedReason !== null;
    const onBreak = timer.pausedReason === "break";

    return (
      <div className="flex h-full flex-col">
        <div className="titlebar-drag h-11 shrink-0" />

        <div className="flex flex-1 flex-col items-center justify-center px-8 pb-16">
          <div className="animate-in flex flex-col items-center">
            <div className="flex items-center gap-2 text-[14px] text-[var(--ink-dim)]">
              <ColourDot colour={timer.subjectColour} size={9} />
              {timer.subjectName}
              {timer.topicName && (
                <span className="text-[var(--ink-faint)]">· {timer.topicName}</span>
              )}
            </div>

            <div
              className={cx(
                "tabular mt-5 text-[80px] font-light leading-none tracking-[-0.03em] transition-colors duration-300",
                paused ? "text-[var(--ink-faint)]" : "text-[var(--ink)]",
              )}
            >
              {clock(timer.activeSeconds)}
            </div>

            <div className="mt-3 h-5 text-[13px] text-[var(--ink-faint)]">
              {onBreak ? (
                <span className="flex items-center gap-1.5 text-[var(--color-positive)]">
                  <Coffee size={14} />
                  Break
                  {timer.phaseRemainingSeconds != null &&
                    ` · ${clock(timer.phaseRemainingSeconds)} left`}
                </span>
              ) : timer.pausedReason === "idle" ? (
                // Framed as the app doing its job, not as an accusation.
                <span>Paused — no activity. It'll pick back up when you do.</span>
              ) : timer.pausedReason === "manual" ? (
                <span>Paused</span>
              ) : timer.mode === "pomodoro" && timer.phaseRemainingSeconds != null ? (
                <span>
                  {clock(timer.phaseRemainingSeconds)} left in this block
                  {timer.completedWorkBlocks > 0 && ` · ${timer.completedWorkBlocks} done`}
                </span>
              ) : (
                <span>Counting active time only</span>
              )}
            </div>

            <div className="mt-9 flex items-center gap-2.5">
              {paused && !onBreak ? (
                <Button size="lg" variant="primary" onClick={() => api.resumeTimer().then(setTimer)}>
                  <Play size={16} />
                  Resume
                </Button>
              ) : (
                <Button size="lg" onClick={() => api.pauseTimer().then(setTimer)} disabled={onBreak}>
                  <Pause size={16} />
                  Pause
                </Button>
              )}
              <Button size="lg" variant="danger" onClick={stop}>
                <Square size={15} />
                Stop
              </Button>
            </div>

            {/* Visible friction, as the brief asks — stated as a fact, without
                comment. The number is the point; a scolding sentence isn't. */}
            {timer.pauseCount > 0 && (
              <div className="mt-8 text-[12.5px] text-[var(--ink-faint)]">
                {timer.pauseCount} {timer.pauseCount === 1 ? "pause" : "pauses"} this session
                {timer.idlePauseCount > 0 && ` · ${timer.idlePauseCount} from inactivity`}
              </div>
            )}

            {timer.elapsedSeconds - timer.activeSeconds > 60 && (
              <div className="mt-1.5 text-[12.5px] text-[var(--ink-faint)]">
                {duration(timer.elapsedSeconds - timer.activeSeconds)} not counted
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ---- Setup ------------------------------------------------------------
  return (
    <div className="flex h-full flex-col">
      <div className="titlebar-drag h-11 shrink-0" />

      <div className="flex flex-1 items-center justify-center px-8 pb-16">
        <div className="animate-in w-full max-w-[440px]">
          <h1 className="text-[24px] font-semibold tracking-[-0.025em]">Start a session</h1>
          <p className="mt-1.5 text-[13.5px] text-[var(--ink-dim)]">
            The timer pauses itself after two minutes without input, so what it records is time you
            were actually there.
          </p>

          <Card className="mt-6 p-5">
            <div className="text-[11px] font-semibold uppercase tracking-[0.07em] text-[var(--ink-faint)]">
              Subject
            </div>
            <div className="mt-2.5 flex flex-wrap gap-1.5">
              {subjects.map((s) => (
                <button
                  key={s.id}
                  onClick={() => setSubjectId(s.id)}
                  className={cx(
                    "flex items-center gap-2 rounded-full border px-3 py-1.5 text-[13px] transition-all duration-[120ms] active:scale-[0.97]",
                    subjectId === s.id
                      ? "border-[var(--ink-faint)] bg-[var(--surface-hi)] text-[var(--ink)]"
                      : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)]",
                  )}
                >
                  <ColourDot colour={s.colour} size={8} />
                  {s.name}
                </button>
              ))}
            </div>

            <div className="mt-5 text-[11px] font-semibold uppercase tracking-[0.07em] text-[var(--ink-faint)]">
              Mode
            </div>
            <div className="mt-2.5">
              <Segmented
                value={mode}
                onChange={setMode}
                options={[
                  { value: "stopwatch", label: "Stopwatch" },
                  { value: "pomodoro", label: "Pomodoro" },
                ]}
              />
            </div>

            {mode === "pomodoro" && (
              <div className="animate-in mt-4">
                <div className="flex flex-wrap items-center gap-1.5">
                  {PRESETS.map((p) => (
                    <button
                      key={p.label}
                      onClick={() => {
                        setWork(p.work);
                        setBrk(p.break);
                      }}
                      className={cx(
                        "rounded-full border px-3 py-1.5 text-[12.5px] transition-all duration-[120ms]",
                        work === p.work && brk === p.break
                          ? "border-[var(--ink-faint)] bg-[var(--surface-hi)] text-[var(--ink)]"
                          : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)]",
                      )}
                    >
                      {p.label}
                    </button>
                  ))}
                  <div className="flex items-center gap-1.5 text-[12.5px] text-[var(--ink-faint)]">
                    <NumberBox value={work} onChange={setWork} min={1} max={180} />
                    <span>/</span>
                    <NumberBox value={brk} onChange={setBrk} min={1} max={60} />
                    <span>min</span>
                  </div>
                </div>
              </div>
            )}

            <p className="mt-5 border-t border-[var(--line-soft)] pt-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
              Topic tagging arrives with the VCAA topic tree, in a later checkpoint.
            </p>
          </Card>

          {error && <div className="mt-3 text-[13px] text-[var(--danger)]">{error}</div>}

          <Button
            size="lg"
            variant="primary"
            className="mt-5 w-full"
            disabled={subjectId == null}
            onClick={start}
          >
            <Play size={16} />
            Start
          </Button>
        </div>
      </div>
    </div>
  );
}

function NumberBox({
  value,
  onChange,
  min,
  max,
}: {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
}) {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      onChange={(e) => {
        const n = Number(e.target.value);
        if (!Number.isNaN(n)) onChange(Math.min(max, Math.max(min, n)));
      }}
      className="tabular h-7 w-[52px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[12.5px] text-[var(--ink)]"
    />
  );
}

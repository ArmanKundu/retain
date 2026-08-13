// The focus dock.
//
// A running session used to be visible only as a chip in the sidebar or by
// navigating to the Timer screen. Neither is right: while a session is running
// it's the most important state in the app, but it shouldn't take the whole
// window either. So it becomes a small floating instrument that follows you
// across every screen and stays out of the way.
//
// It drives the same three Tauri commands the Timer screen does — pause, resume,
// stop — and reads the same `timer:tick` snapshot from the store. No new
// backend, no second source of truth.

import { useState } from "react";
import { Pause, Play, Square } from "lucide-react";

import { api } from "../lib/api";
import { clock } from "../lib/format";
import type { FinishedSession } from "../lib/types";
import { useApp } from "../store";
import { FloatingDock } from "./primitives";
import { cx } from "./ui";

export function FocusDock({ onFinished }: { onFinished: (s: FinishedSession) => void }) {
  const timer = useApp((s) => s.timer);
  const setTimer = useApp((s) => s.setTimer);
  const route = useApp((s) => s.route);
  const setRoute = useApp((s) => s.setRoute);
  const refreshProgress = useApp((s) => s.refreshProgress);

  const [busy, setBusy] = useState(false);

  // The Timer screen already shows all of this at full size; a dock on top of
  // it would be the same information twice.
  if (!timer || route === "timer") return null;

  const paused = timer.pausedReason != null;

  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
    } finally {
      setBusy(false);
    }
  };

  return (
    <FloatingDock>
      {/* Subject: colour as a small accent, never a fill. */}
      <button
        onClick={() => setRoute("timer")}
        title="Open the timer"
        className="pressable flex min-w-0 items-center gap-2.5 rounded-[var(--r-sm)] pr-1 text-left"
      >
        <span
          className="relative flex h-2 w-2 shrink-0 rounded-full"
          style={{ background: timer.subjectColour }}
        >
          {/* A slow pulse only while actually running — a paused session that
              still pulses reads as running, which is the one thing this must
              never get wrong. */}
          {!paused && (
            <span
              className="absolute inset-0 animate-ping rounded-full opacity-60"
              style={{ background: timer.subjectColour, animationDuration: "2.4s" }}
            />
          )}
        </span>
        <span className="max-w-[140px] truncate text-[13px] text-[var(--ink-dim)]">
          {timer.subjectName}
        </span>
      </button>

      <div
        className={cx(
          "tabular text-[22px] font-medium leading-none tracking-[-0.02em] transition-opacity duration-[var(--t-base)]",
          paused ? "text-[var(--ink-dim)] opacity-70" : "text-[var(--ink)]",
        )}
      >
        {clock(timer.activeSeconds)}
      </div>

      {paused && (
        <span className="rounded-full border border-[var(--line)] px-2 py-0.5 text-[11px] text-[var(--ink-faint)]">
          {timer.pausedReason === "idle"
            ? "idle"
            : timer.pausedReason === "break"
              ? "break"
              : "paused"}
        </span>
      )}

      <div className="ml-1 flex items-center gap-1.5">
        <DockButton
          label={paused ? "Resume session" : "Pause session"}
          disabled={busy}
          onClick={() =>
            void act(async () => setTimer(paused ? await api.resumeTimer() : await api.pauseTimer()))
          }
        >
          {paused ? <Play size={14} /> : <Pause size={14} />}
        </DockButton>

        <DockButton
          label="Stop and log this session"
          disabled={busy}
          danger
          onClick={() =>
            void act(async () => {
              const finished = await api.stopTimer();
              setTimer(null);
              if (finished) onFinished(finished);
              await refreshProgress();
            })
          }
        >
          <Square size={12} />
        </DockButton>
      </div>
    </FloatingDock>
  );
}

function DockButton({
  children,
  label,
  onClick,
  disabled,
  danger,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      className={cx(
        "pressable flex h-8 w-8 items-center justify-center rounded-full",
        "border border-[var(--line)] bg-[var(--surface-hi)]/70",
        "disabled:opacity-40",
        danger
          ? "text-[var(--ink-dim)] hover:border-[var(--danger)] hover:text-[var(--danger)]"
          : "text-[var(--ink)] hover:border-[var(--ink-faint)]",
      )}
    >
      {children}
    </button>
  );
}

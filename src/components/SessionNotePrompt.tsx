import { useEffect, useRef, useState } from "react";

import { api } from "../lib/api";
import { duration } from "../lib/format";
import type { FinishedSession } from "../lib/types";
import { Button } from "./ui";

/**
 * The one-line note, offered after every session.
 *
 * Optional and dismissible, per the brief — Escape or "Skip" closes it and the
 * session is saved either way. The note is the only thing at stake here, never
 * the session itself.
 */
export function SessionNotePrompt({
  session,
  onClose,
}: {
  session: FinishedSession;
  onClose: () => void;
}) {
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const save = async () => {
    setSaving(true);
    try {
      await api.setSessionNote(session.sessionId, note.trim() || null);
    } finally {
      onClose();
    }
  };

  const notCounted = session.elapsedSeconds - session.activeSeconds;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 px-8 backdrop-blur-[2px]"
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
        if (e.key === "Enter") void save();
      }}
    >
      <div className="glass animate-pop w-full max-w-[440px] rounded-[var(--r-xl)] p-6">
        <div className="text-[17px] font-semibold tracking-[-0.01em]">
          {duration(session.activeSeconds)} on {session.subjectName}
        </div>

        <div className="mt-1.5 text-[13px] leading-relaxed text-[var(--ink-dim)]">
          {session.qualifiesForStreak ? "That's today earned." : "Logged."}
          {session.pauseCount > 0 && (
            <>
              {" "}
              {session.pauseCount} {session.pauseCount === 1 ? "pause" : "pauses"}
              {session.idlePauseCount > 0 && `, ${session.idlePauseCount} from inactivity`}.
            </>
          )}
          {notCounted > 60 && ` ${duration(notCounted)} wasn't counted.`}
        </div>

        <input
          ref={inputRef}
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder="did ch7 questions, stuck on titration"
          maxLength={200}
          className="mt-5 h-10 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[14px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] transition-colors focus:border-[var(--accent)]"
        />

        <div className="mt-2 text-[12px] text-[var(--ink-faint)]">
          One line on what you did or where you got stuck. Optional.
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Skip
          </Button>
          <Button variant="primary" onClick={save} disabled={saving}>
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import { Trash2 } from "lucide-react";

import { api } from "../lib/api";
import { duration } from "../lib/format";
import type { FinishedSession } from "../lib/types";
import { Button, cx } from "./ui";
import { useApp } from "../store";

/**
 * What happens when you stop the clock.
 *
 * Two decisions, in the order you actually make them: *what did I do?*, then
 * *does this count?* The second one is the reason this dialog earns its place.
 * Not every timer run is study — you start one, get pulled away, and come back
 * to twenty minutes that would quietly inflate your week. A tracker whose
 * numbers you don't believe is one you stop reading, so discarding is offered
 * as plainly as keeping.
 *
 * Escape keeps the session with whatever note is typed. Discarding is the only
 * destructive path and it takes a deliberate click.
 */
export function SessionNotePrompt({
  session,
  onClose,
}: {
  session: FinishedSession;
  onClose: () => void;
}) {
  const refreshProgress = useApp((s) => s.refreshProgress);

  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmingDiscard, setConfirmingDiscard] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const keep = async () => {
    setBusy(true);
    try {
      await api.setSessionNote(session.sessionId, note.trim() || null);
      await refreshProgress();
    } finally {
      onClose();
    }
  };

  const discard = async () => {
    setBusy(true);
    try {
      await api.discardSession(session.sessionId);
      await refreshProgress();
    } finally {
      onClose();
    }
  };

  const notCounted = session.elapsedSeconds - session.activeSeconds;

  return (
    <div
      className="scrim fixed inset-0 z-50 flex items-center justify-center px-8"
      onKeyDown={(e) => {
        if (e.key === "Escape") void keep();
        // ⌘↵ keeps, so the note can hold newlines without submitting.
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void keep();
      }}
    >
      <div className="sheet animate-pop w-full max-w-[460px] p-7">
        <div className="flex items-baseline gap-2.5">
          <span className="tabular text-[30px] font-semibold leading-none tracking-[var(--track-display)]">
            {duration(session.activeSeconds)}
          </span>
          <span className="text-[15px] text-[var(--ink-dim)]">
            on {session.subjectName}
          </span>
        </div>

        <div className="mt-2 text-[13px] leading-relaxed text-[var(--ink-dim)]">
          {session.qualifiesForStreak ? "That's today earned." : "Nice work."}
          {session.pauseCount > 0 && (
            <>
              {" "}
              {session.pauseCount}{" "}
              {session.pauseCount === 1 ? "pause" : "pauses"}
              {session.idlePauseCount > 0 &&
                `, ${session.idlePauseCount} from inactivity`}
              {notCounted > 0 && ` — ${duration(notCounted)} not counted`}.
            </>
          )}
        </div>

        <label className="mt-5 block text-[13px] font-medium">
          What did you work on?
        </label>
        <textarea
          ref={inputRef}
          value={note}
          onChange={(e) => setNote(e.target.value)}
          rows={3}
          placeholder="Optional. In a week this is what tells you whether the time was well spent."
          className="mt-2 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 text-[13.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
        />

        {confirmingDiscard ? (
          <div className="animate-rise mt-5 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_28%,transparent)] bg-[color-mix(in_srgb,var(--danger)_8%,transparent)] p-4">
            <p className="text-[13px] leading-relaxed text-[var(--ink)]">
              Throw away {duration(session.activeSeconds)}? It won't count
              towards today or your week.
            </p>
            <div className="mt-3 flex gap-2">
              <Button
                size="sm"
                variant="danger"
                disabled={busy}
                onClick={() => void discard()}
              >
                Yes, discard it
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setConfirmingDiscard(false)}
              >
                Keep it
              </Button>
            </div>
          </div>
        ) : (
          <div className="mt-5 flex items-center gap-2">
            <button
              onClick={() => setConfirmingDiscard(true)}
              disabled={busy}
              className={cx(
                "pressable flex items-center gap-1.5 rounded-[var(--r-sm)] px-2 py-1.5",
                "text-[12.5px] text-[var(--ink-faint)]",
                "hover:bg-[color-mix(in_srgb,var(--danger)_9%,transparent)] hover:text-[var(--danger)]",
              )}
            >
              <Trash2 size={13} />
              Don't log this
            </button>

            <Button
              variant="primary"
              className="ml-auto"
              disabled={busy}
              onClick={() => void keep()}
            >
              {busy ? "Saving…" : "Log it"}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}

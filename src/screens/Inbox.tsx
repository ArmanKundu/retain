import { useCallback, useEffect, useState } from "react";
import { Check, Inbox as InboxIcon, Trash2 } from "lucide-react";

import { AiAction, useAi } from "../components/Ai";
import { Button, Card, ColourDot, Empty, SectionTitle, cx } from "../components/ui";
import { api } from "../lib/api";
import { prettyDate } from "../lib/format";
import type { Capture, Task } from "../lib/types";
import { useApp } from "../store";

/**
 * The capture inbox and the tasks it becomes.
 *
 * Triage is deliberately a human step. The parser's guesses arrive pre-filled
 * and editable, but nothing is filed until you say so — a capture that silently
 * assigned itself the wrong subject and date is how you stop trusting the inbox,
 * and an inbox you don't trust is one you stop using.
 */
export function Inbox() {
  const subjects = useApp((s) => s.subjects);
  const [captures, setCaptures] = useState<Capture[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [showDone, setShowDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [inbox, list] = await Promise.all([api.listInbox(), api.listTasks(showDone)]);
      setCaptures(inbox);
      setTasks(list);
    } catch (e) {
      setError(String(e));
    }
  }, [showDone]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 sm:px-9 pb-14">
      <div className="titlebar-drag h-11" />

      <header className="mb-6">
        <h1 className="text-[24px] font-semibold tracking-[-0.025em]">Inbox</h1>
        <p className="mt-1 text-[13.5px] text-[var(--ink-dim)]">
          ⌘⇧Space captures from anywhere. Sort it out here when you have a moment.
        </p>
      </header>

      {error && (
        <div className="mb-4 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] px-4 py-3 text-[13px] text-[var(--danger)]">
          {error}
        </div>
      )}

      {captures.length > 0 && (
        <section className="mb-7">
          <SectionTitle>To triage — {captures.length}</SectionTitle>
          <div className="mt-2.5 space-y-2.5">
            {captures.map((c) => (
              <TriageRow key={c.id} capture={c} subjects={subjects} onDone={load} />
            ))}
          </div>
        </section>
      )}

      <section>
        <div className="flex items-center gap-3">
          <SectionTitle>Tasks</SectionTitle>
          <button
            onClick={() => setShowDone(!showDone)}
            className="ml-auto text-[12px] text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            {showDone ? "Hide done" : "Show done"}
          </button>
        </div>

        <Card className="mt-2.5">
          {tasks.length === 0 ? (
            captures.length === 0 ? (
              <div className="flex flex-col items-center px-6 py-14 text-center">
                <InboxIcon size={22} className="mb-3 text-[var(--ink-faint)]" />
                <div className="text-[14px] font-medium text-[var(--ink-dim)]">Inbox empty</div>
                <div className="mt-1.5 max-w-[380px] text-[13px] leading-relaxed text-[var(--ink-faint)]">
                  Press ⌘⇧Space from any app and type. It saves and disappears — sort it out later.
                </div>
              </div>
            ) : (
              <Empty title="No tasks yet" body="Triage a capture above to make your first one." />
            )
          ) : (
            <div className="divide-y divide-[var(--line-soft)]">
              {tasks.map((t) => (
                <div
                  key={t.id}
                  className="group flex items-center gap-3 px-5 py-3 transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/50"
                >
                  <button
                    onClick={async () => {
                      await api.setTaskDone(t.id, !t.doneAt);
                      await load();
                    }}
                    aria-label={t.doneAt ? `Mark ${t.title} not done` : `Mark ${t.title} done`}
                    className={cx(
                      "pressable flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-[7px] border",
                      t.doneAt
                        ? "border-[var(--color-positive)] bg-[var(--color-positive)] text-white"
                        : "border-[var(--line)] hover:border-[var(--ink-faint)]",
                    )}
                  >
                    {t.doneAt && <Check size={12} strokeWidth={3} />}
                  </button>

                  <div className="min-w-0 flex-1">
                    <div
                      className={cx(
                        "selectable truncate text-[13.5px]",
                        t.doneAt ? "text-[var(--ink-faint)] line-through" : "text-[var(--ink)]",
                      )}
                    >
                      {t.title}
                    </div>
                    <div className="flex items-center gap-2 text-[11.5px] text-[var(--ink-faint)]">
                      {t.subjectName && (
                        <span className="flex items-center gap-1">
                          <ColourDot colour={t.colour ?? "#888"} size={6} />
                          {t.subjectName}
                        </span>
                      )}
                      {t.dueOn && <span>{prettyDate(t.dueOn)}</span>}
                    </div>
                  </div>

                  <button
                    onClick={async () => {
                      await api.deleteTask(t.id);
                      await load();
                    }}
                    aria-label={`Delete ${t.title}`}
                    className={cx(
                      "pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)]",
                      "opacity-0 transition-all duration-[var(--t-fast)]",
                      "hover:bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] hover:text-[var(--danger)]",
                      // Revealed on row hover, but always reachable by keyboard —
                      // a control that only exists on hover is invisible to
                      // anyone tabbing through.
                      "group-hover:opacity-100 focus-visible:opacity-100",
                    )}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </Card>
      </section>
    </div>
  );
}

function TriageRow({
  capture,
  subjects,
  onDone,
}: {
  capture: Capture;
  subjects: ReturnType<typeof useApp.getState>["subjects"];
  onDone: () => Promise<void>;
}) {
  // Suggestions pre-fill the form. They are the starting point, not the answer.
  const [title, setTitle] = useState(capture.suggestedTitle ?? capture.rawText);
  const [subjectId, setSubjectId] = useState<number | null>(capture.suggestedSubjectId);
  const [dueOn, setDueOn] = useState(capture.suggestedDueOn ?? "");
  const [busy, setBusy] = useState(false);
  const { enabled: aiEnabled } = useAi();

  const makeTask = async () => {
    setBusy(true);
    try {
      await api.triageCaptureToTask(capture.id, title, subjectId, dueOn || null);
      await onDone();
    } finally {
      setBusy(false);
    }
  };

  const discard = async () => {
    setBusy(true);
    try {
      await api.triageCapture(capture.id, "discarded");
      await onDone();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="animate-in p-4">
      {/* The raw line, always visible — the parse never replaces what you typed. */}
      <div className="selectable font-mono text-[12px] text-[var(--ink-faint)]">
        {capture.rawText}
      </div>

      <input
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        className="mt-2 h-8 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13.5px] text-[var(--ink)] focus:border-[var(--accent)]"
      />

      <div className="mt-2.5 flex flex-wrap items-center gap-2">
        <select
          value={subjectId ?? ""}
          onChange={(e) => setSubjectId(e.target.value ? Number(e.target.value) : null)}
          className="h-7 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12px] text-[var(--ink)]"
        >
          <option value="">No subject</option>
          {subjects.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>

        <input
          type="date"
          value={dueOn}
          onChange={(e) => setDueOn(e.target.value)}
          className="h-7 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12px] text-[var(--ink)]"
        />

        <Button size="sm" variant="primary" disabled={busy || !title.trim()} onClick={makeTask}>
          Make task
        </Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={discard}>
          Discard
        </Button>

        {/* Fills the same three fields the offline parser fills, for the messy
            lines it can't crack. It only ever re-fills the form — the task is
            still made by pressing the button above. */}
        {aiEnabled && (
          <AiAction
            className="ml-auto"
            label="Tidy up"
            run={() => api.aiTaskFromNote(capture.rawText)}
            onDone={(s) => {
              if (s.title.trim()) setTitle(s.title.trim());
              if (s.dueOn) setDueOn(s.dueOn);
              const match = subjects.find(
                (x) => x.name.toLowerCase() === (s.subject ?? "").toLowerCase(),
              );
              if (match) setSubjectId(match.id);
            }}
          />
        )}
      </div>
    </Card>
  );
}

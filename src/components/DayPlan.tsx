// Today's plan, and what happened to yesterday's.
//
// The interesting part of this component isn't the checklist — it's the notice
// above it. When Retain moves work forward it says so, in a line you can read
// and undo, because a planner that silently rearranges itself is one you stop
// being able to trust. "Chemistry moved to today" is information. A Wednesday
// that quietly grew two extra items is not.
//
// Nothing here decides where work goes. That's `plan::rollover` in Rust, which
// is deterministic and capacity-aware; this file only shows the result.

import { useCallback, useEffect, useState } from "react";
import {
  Check,
  CircleAlert,
  Plus,
  RotateCw,
  SkipForward,
  X,
} from "lucide-react";

import { api } from "../lib/api";
import type { PlanItem, Rollover, Subject } from "../lib/types";
import { Button, cx } from "./ui";
import { SectionHeader } from "./primitives";

/** `YYYY-MM-DD` for a date, in local time — never `toISOString`, which is UTC. */
function isoOf(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

/** "Tuesday", or "the 3rd" once it's more than a week back. */
function whenLabel(iso: string, today: string): string {
  const from = new Date(`${iso}T12:00:00`);
  const now = new Date(`${today}T12:00:00`);
  const days = Math.round((now.getTime() - from.getTime()) / 86400000);

  if (days === 1) return "yesterday";
  if (days > 1 && days <= 6)
    return from.toLocaleDateString(undefined, { weekday: "long" });
  return from.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

export function DayPlan({ subjects }: { subjects: Subject[] }) {
  const today = isoOf(new Date());

  const [items, setItems] = useState<PlanItem[]>([]);
  const [rolled, setRolled] = useState<Rollover | null>(null);
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setItems(await api.planForDate(today));
    } catch {
      setItems([]);
    }
  }, [today]);

  useEffect(() => {
    // The backend already rolled at launch; this pass is the one that reports
    // it, and it's a no-op if the window was opened twice.
    void (async () => {
      try {
        const result = await api.runRollover(false);
        if (result.moved.length > 0 || result.stuck.length > 0)
          setRolled(result);
      } catch {
        // A rollover failure costs the notice, not the list.
      }
      await load();
    })();
  }, [load]);

  const reshuffle = async () => {
    setBusy(true);
    try {
      setRolled(await api.runRollover(true));
      await load();
    } finally {
      setBusy(false);
    }
  };

  const setStatus = async (
    item: PlanItem,
    status: "planned" | "done" | "skipped",
  ) => {
    // Optimistic: ticking something off should feel instant.
    setItems((cur) =>
      cur.map((i) => (i.id === item.id ? { ...i, status } : i)),
    );
    try {
      await api.setPlanStatus(item.id, status);
    } catch {
      await load();
    }
  };

  const outstanding = items.filter((i) => i.status === "planned");
  const settled = items.filter((i) => i.status !== "planned");

  return (
    <section className="animate-rise mb-9">
      <SectionHeader
        title="Today's plan"
        hint={
          outstanding.length > 0
            ? `${outstanding.reduce((n, i) => n + i.estMinutes, 0)} min across ${
                outstanding.length
              } ${outstanding.length === 1 ? "thing" : "things"}`
            : items.length > 0
              ? "all clear"
              : undefined
        }
      >
        <button
          onClick={() => void reshuffle()}
          disabled={busy}
          title="Re-fit the plan around your commitments"
          className="pressable flex items-center gap-1.5 rounded-full border border-[var(--line)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)] hover:text-[var(--ink)] disabled:opacity-50"
        >
          <RotateCw size={12} className={busy ? "animate-spin" : undefined} />
          Reshuffle
        </button>
        <button
          onClick={() => setAdding((v) => !v)}
          className="pressable flex items-center gap-1.5 rounded-full border border-[var(--line)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
        >
          <Plus size={12} />
          Add
        </button>
      </SectionHeader>

      {rolled && (
        <RolloverNotice
          result={rolled}
          today={today}
          onDismiss={() => setRolled(null)}
        />
      )}

      {adding && (
        <AddRow
          subjects={subjects}
          today={today}
          onDone={async () => {
            setAdding(false);
            await load();
          }}
          onCancel={() => setAdding(false)}
        />
      )}

      {items.length === 0 && !adding ? (
        <p className="px-1 text-[13.5px] leading-relaxed text-[var(--ink-dim)]">
          Nothing planned. Add what you mean to get through — anything you don't
          reach moves itself to the next day that has room.
        </p>
      ) : (
        <ul className="space-y-1.5">
          {[...outstanding, ...settled].map((item) => (
            <PlanRow
              key={item.id}
              item={item}
              today={today}
              onStatus={setStatus}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function PlanRow({
  item,
  today,
  onStatus,
}: {
  item: PlanItem;
  today: string;
  onStatus: (item: PlanItem, status: "planned" | "done" | "skipped") => void;
}) {
  const done = item.status === "done";
  const skipped = item.status === "skipped";
  const settled = done || skipped;

  return (
    <li
      className={cx(
        "group flex items-center gap-3 rounded-[var(--r-md)] border px-3.5 py-2.5 transition-colors duration-[var(--t-fast)]",
        settled
          ? "border-transparent bg-transparent"
          : "border-[var(--line-soft)] bg-[var(--surface)] hover:border-[var(--line)]",
      )}
    >
      <button
        onClick={() => onStatus(item, done ? "planned" : "done")}
        aria-label={done ? "Mark as not done" : "Mark as done"}
        className={cx(
          "pressable grid h-[19px] w-[19px] shrink-0 place-items-center rounded-full border transition-colors duration-[var(--t-fast)]",
          done
            ? "border-transparent bg-[var(--color-positive)] text-white"
            : "border-[var(--ink-faint)] text-transparent hover:border-[var(--accent)]",
        )}
      >
        <Check size={11} strokeWidth={3} />
      </button>

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          {item.colour && !settled && (
            <span
              aria-hidden
              className="h-[7px] w-[7px] shrink-0 rounded-full"
              style={{ background: item.colour }}
            />
          )}
          <span
            className={cx(
              "truncate text-[13.5px]",
              settled
                ? "text-[var(--ink-faint)] line-through"
                : "text-[var(--ink)]",
            )}
          >
            {item.title}
          </span>
          <span className="shrink-0 text-[11.5px] text-[var(--ink-faint)]">
            {item.estMinutes}m
          </span>
        </div>

        {/* The honest line. Shown from the second move, because one slipped day
            is normal and three is a plan that needs changing, not more effort. */}
        {!settled && item.moves >= 2 && (
          <div className="mt-0.5 text-[11.5px] text-[var(--warn)]">
            Moved {item.moves} times since{" "}
            {whenLabel(item.firstPlannedOn, today)}
          </div>
        )}
        {!settled && item.dueOn && (
          <div className="mt-0.5 text-[11.5px] text-[var(--ink-faint)]">
            For{" "}
            {new Date(`${item.dueOn}T12:00:00`).toLocaleDateString(undefined, {
              weekday: "short",
              day: "numeric",
              month: "short",
            })}
          </div>
        )}
      </div>

      {!settled && (
        <button
          onClick={() => onStatus(item, "skipped")}
          title="Not doing this"
          className="pressable shrink-0 rounded-full p-1 text-[var(--ink-faint)] opacity-0 hover:text-[var(--ink-dim)] group-hover:opacity-100"
        >
          <SkipForward size={13} />
        </button>
      )}
      {skipped && (
        <button
          onClick={() => onStatus(item, "planned")}
          className="pressable shrink-0 text-[11.5px] text-[var(--ink-faint)] hover:text-[var(--ink-dim)]"
        >
          Put back
        </button>
      )}
    </li>
  );
}

/**
 * What moved, and what couldn't.
 *
 * Stuck work is listed separately and more loudly. "There's no room before it's
 * due" is a decision the app deliberately refuses to make for you.
 */
function RolloverNotice({
  result,
  today,
  onDismiss,
}: {
  result: Rollover;
  today: string;
  onDismiss: () => void;
}) {
  return (
    <div className="animate-rise mb-3 rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] px-4 py-3">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          {result.moved.length > 0 && (
            <>
              <div className="text-[13px] font-medium text-[var(--ink)]">
                {result.moved.length === 1
                  ? "One thing moved forward"
                  : `${result.moved.length} things moved forward`}
              </div>
              <ul className="mt-1 space-y-0.5">
                {result.moved.map((m) => (
                  <li
                    key={m.id}
                    className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]"
                  >
                    {m.subjectName ? `${m.subjectName}: ` : ""}
                    {m.title} — from {whenLabel(m.from, today)} to{" "}
                    {m.to === today ? "today" : whenLabel(m.to, today)}
                  </li>
                ))}
              </ul>
            </>
          )}

          {result.stuck.length > 0 && (
            <div className={result.moved.length > 0 ? "mt-3" : undefined}>
              <div className="flex items-center gap-1.5 text-[13px] font-medium text-[var(--warn)]">
                <CircleAlert size={13} />
                {result.stuck.length === 1
                  ? "One thing had nowhere to go"
                  : `${result.stuck.length} things had nowhere to go`}
              </div>
              <ul className="mt-1 space-y-0.5">
                {result.stuck.map((s) => (
                  <li
                    key={s.id}
                    className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]"
                  >
                    {s.subjectName ? `${s.subjectName}: ` : ""}
                    {s.title} — {s.reason}
                  </li>
                ))}
              </ul>
              <p className="mt-1.5 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
                Left where it was. Shorten it, drop it, or clear something in
                your week.
              </p>
            </div>
          )}
        </div>

        <button
          onClick={onDismiss}
          aria-label="Dismiss"
          className="pressable shrink-0 rounded-full p-1 text-[var(--ink-faint)] hover:text-[var(--ink)]"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}

const PRESETS = [20, 30, 45, 60, 90];

function AddRow({
  subjects,
  today,
  onDone,
  onCancel,
}: {
  subjects: Subject[];
  today: string;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [title, setTitle] = useState("");
  const [subjectId, setSubjectId] = useState<number | null>(
    subjects[0]?.id ?? null,
  );
  const [minutes, setMinutes] = useState(30);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!title.trim()) return;
    try {
      await api.createPlanItem({
        subjectId,
        title: title.trim(),
        detail: null,
        plannedOn: today,
        estMinutes: minutes,
        dueOn: null,
        source: "manual",
      });
      onDone();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="animate-rise mb-2 rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3.5">
      <input
        autoFocus
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void submit();
          if (e.key === "Escape") onCancel();
        }}
        placeholder="What are you going to do?"
        className="h-9 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface)] px-3 text-[13.5px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
      />

      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        {subjects.map((s) => (
          <button
            key={s.id}
            onClick={() => setSubjectId(s.id === subjectId ? null : s.id)}
            className={cx(
              "pressable rounded-full border px-2.5 py-1 text-[12px]",
              s.id === subjectId
                ? "border-transparent text-white"
                : "border-[var(--line)] text-[var(--ink-dim)] hover:text-[var(--ink)]",
            )}
            style={s.id === subjectId ? { background: s.colour } : undefined}
          >
            {s.name}
          </button>
        ))}
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-2">
        <div className="flex gap-1">
          {PRESETS.map((m) => (
            <button
              key={m}
              onClick={() => setMinutes(m)}
              className={cx(
                "pressable rounded-full border px-2.5 py-1 text-[12px]",
                m === minutes
                  ? "border-[var(--accent)]/40 bg-[var(--accent)]/12 text-[var(--accent)]"
                  : "border-[var(--line)] text-[var(--ink-dim)] hover:text-[var(--ink)]",
              )}
            >
              {m}m
            </button>
          ))}
        </div>

        <div className="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            size="sm"
            disabled={!title.trim()}
            onClick={() => void submit()}
          >
            Add
          </Button>
        </div>
      </div>

      {error && (
        <p className="mt-2 text-[12px] text-[var(--danger)]">{error}</p>
      )}
    </div>
  );
}

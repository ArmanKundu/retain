// The week: what your time actually looks like.
//
// The calendar was a list of class names going down the page, which told you
// what you had but never when you were free — the thing you're actually trying
// to work out on a Sunday night.
//
// This is a time grid. Classes from your Compass feed sit alongside blocks you
// add yourself: tuition, work, family, rest. Every block says whether you can
// study in it, and the free-time figure under each day is computed from that.
// The assistant reads the same numbers, so "what should I do tonight?" accounts
// for tonight already being spoken for.

import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, Plus, Trash2, X } from "lucide-react";

import { SectionHeader } from "../components/primitives";
import { Button, Card, cx } from "../components/ui";
import { api } from "../lib/api";
import type { BlockKind, CalendarEvent, NewBlock, TimeBlock } from "../lib/types";
import { useApp } from "../store";

/** The visible window. Outside this you're asleep or Retain has no business. */
const DAY_START = 7 * 60;
const DAY_END = 22 * 60;
const PX_PER_MIN = 0.82;

const KINDS: { value: BlockKind; label: string; tint: string }[] = [
  { value: "class", label: "Class", tint: "220 70% 55%" },
  { value: "tuition", label: "Tuition", tint: "265 60% 58%" },
  { value: "work", label: "Work", tint: "20 70% 52%" },
  { value: "commute", label: "Travel", tint: "210 12% 50%" },
  { value: "exercise", label: "Exercise", tint: "150 55% 42%" },
  { value: "family", label: "Family", tint: "340 60% 58%" },
  { value: "rest", label: "Rest", tint: "195 45% 48%" },
  { value: "other", label: "Other", tint: "210 10% 48%" },
];

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function tintOf(kind: BlockKind): string {
  return KINDS.find((k) => k.value === kind)?.tint ?? "210 10% 48%";
}

function clock(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  const suffix = h < 12 ? "am" : "pm";
  const display = h % 12 === 0 ? 12 : h % 12;
  return m === 0 ? `${display}${suffix}` : `${display}:${String(m).padStart(2, "0")}${suffix}`;
}

function isoOf(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

/** The Monday on or before a date. */
function mondayOf(d: Date): Date {
  const out = new Date(d);
  out.setDate(out.getDate() - ((out.getDay() + 6) % 7));
  out.setHours(12, 0, 0, 0);
  return out;
}

/**
 * Free minutes in a day, merging overlaps.
 *
 * Mirrors `blocks::free_minutes` in Rust. Two commitments from 4–5 and 4:30–6
 * consume ninety minutes, not a hundred and twenty — summing them would
 * systematically understate your free time, which is what makes a planner feel
 * punishing rather than useful.
 */
function freeMinutes(items: { startMin: number; endMin: number; available: boolean }[]): number {
  const busy = items
    .filter((b) => !b.available)
    .map((b) => [Math.max(b.startMin, DAY_START), Math.min(b.endMin, DAY_END)] as const)
    .filter(([s, e]) => e > s)
    .sort((a, b) => a[0] - b[0]);

  let consumed = 0;
  let cursor = -1;
  let end = -1;

  for (const [s, e] of busy) {
    if (cursor === -1) {
      cursor = s;
      end = e;
    } else if (s <= end) {
      end = Math.max(end, e);
    } else {
      consumed += end - cursor;
      cursor = s;
      end = e;
    }
  }
  if (cursor !== -1) consumed += end - cursor;

  return DAY_END - DAY_START - consumed;
}

export function Week() {
  const subjects = useApp((s) => s.subjects);

  const [anchor, setAnchor] = useState(() => mondayOf(new Date()));
  const [blocks, setBlocks] = useState<TimeBlock[]>([]);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [editing, setEditing] = useState<{ block: TimeBlock | null; weekday: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const days = useMemo(
    () =>
      Array.from({ length: 7 }, (_, i) => {
        const d = new Date(anchor);
        d.setDate(d.getDate() + i);
        return d;
      }),
    [anchor],
  );

  const load = useCallback(async () => {
    try {
      setBlocks(await api.listBlocks());
    } catch (e) {
      setError(String(e));
    }
    // Compass classes sit alongside your own blocks. A failure here costs the
    // classes, not the grid.
    try {
      setEvents(await api.upcomingEvents(30, 200));
    } catch {
      setEvents([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const todayIso = isoOf(new Date());

  return (
    <div className="mx-auto w-full max-w-[min(1200px,100%)] px-6 pb-16 sm:px-9">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-6 flex flex-wrap items-end gap-4">
        <div>
          <h1 className="text-[28px] font-semibold tracking-[-0.028em]">Your week</h1>
          <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
            Mark what your time is already spoken for, and Retain stops suggesting you study then.
          </p>
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => setAnchor((a) => new Date(a.getTime() - 7 * 86400000))}
            aria-label="Previous week"
            className="pressable rounded-[var(--r-sm)] border border-[var(--line)] p-1.5 text-[var(--ink-dim)] hover:text-[var(--ink)]"
          >
            <ChevronLeft size={15} />
          </button>
          <button
            onClick={() => setAnchor(mondayOf(new Date()))}
            className="pressable rounded-[var(--r-sm)] border border-[var(--line)] px-3 py-1.5 text-[12.5px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
          >
            This week
          </button>
          <button
            onClick={() => setAnchor((a) => new Date(a.getTime() + 7 * 86400000))}
            aria-label="Next week"
            className="pressable rounded-[var(--r-sm)] border border-[var(--line)] p-1.5 text-[var(--ink-dim)] hover:text-[var(--ink)]"
          >
            <ChevronRight size={15} />
          </button>
        </div>
      </header>

      {error && <p className="mb-3 text-[12.5px] text-[var(--danger)]">{error}</p>}

      <Card className="animate-rise overflow-hidden p-0">
        <div className="flex">
          {/* Hour labels */}
          <div className="w-[52px] shrink-0 pt-[46px]">
            {Array.from({ length: (DAY_END - DAY_START) / 60 + 1 }, (_, i) => (
              <div
                key={i}
                className="relative text-right text-[10.5px] text-[var(--ink-faint)]"
                style={{ height: 60 * PX_PER_MIN }}
              >
                <span className="absolute right-2 -top-[6px]">{clock(DAY_START + i * 60)}</span>
              </div>
            ))}
          </div>

          <div className="flex min-w-0 flex-1">
            {days.map((day) => {
              const iso = isoOf(day);
              const weekday = (day.getDay() + 6) % 7;
              const isToday = iso === todayIso;

              const dayBlocks = blocks.filter(
                (b) => b.weekday === weekday || b.onDate === iso,
              );
              const dayEvents = events.filter((e) => e.localDate === iso && !e.allDay);

              // Compass classes count as committed time too.
              const free = freeMinutes([
                ...dayBlocks,
                ...dayEvents.map((e) => ({
                  startMin: minutesOf(e.startsAt),
                  endMin: e.endsAt ? minutesOf(e.endsAt) : minutesOf(e.startsAt) + 50,
                  available: false,
                })),
              ]);

              return (
                <div
                  key={iso}
                  className="min-w-0 flex-1 border-l border-[var(--line-soft)] first:border-l-0"
                >
                  <div
                    className={cx(
                      "px-2 pb-2 pt-3 text-center",
                      isToday && "bg-[var(--accent)]/6",
                    )}
                  >
                    <div
                      className={cx(
                        "text-[11.5px]",
                        isToday ? "text-[var(--accent)]" : "text-[var(--ink-faint)]",
                      )}
                    >
                      {WEEKDAYS[weekday]}
                    </div>
                    <div
                      className={cx(
                        "tabular text-[15px] font-medium",
                        isToday ? "text-[var(--accent)]" : "text-[var(--ink)]",
                      )}
                    >
                      {day.getDate()}
                    </div>
                    <div className="mt-0.5 text-[10.5px] text-[var(--ink-faint)]">
                      {Math.floor(free / 60)}h free
                    </div>
                  </div>

                  {/* The column. Clicking empty space adds a block there. */}
                  <div
                    className="relative border-t border-[var(--line-soft)]"
                    style={{ height: (DAY_END - DAY_START) * PX_PER_MIN }}
                    onClick={(e) => {
                      const rect = e.currentTarget.getBoundingClientRect();
                      const minute =
                        DAY_START + Math.round((e.clientY - rect.top) / PX_PER_MIN / 30) * 30;
                      setEditing({
                        block: {
                          id: 0,
                          title: "",
                          kind: "other",
                          weekday,
                          onDate: null,
                          startMin: Math.min(minute, DAY_END - 60),
                          endMin: Math.min(minute + 60, DAY_END),
                          available: false,
                          subjectId: null,
                          subjectName: null,
                          colour: null,
                          note: null,
                        },
                        weekday,
                      });
                    }}
                  >
                    {/* Hour lines */}
                    {Array.from({ length: (DAY_END - DAY_START) / 60 }, (_, i) => (
                      <div
                        key={i}
                        className="absolute left-0 right-0 border-t border-[var(--line-soft)]"
                        style={{ top: (i + 1) * 60 * PX_PER_MIN }}
                      />
                    ))}

                    {/* Compass classes, read-only — they come from the feed. */}
                    {dayEvents.map((e) => {
                      const start = minutesOf(e.startsAt);
                      const end = e.endsAt ? minutesOf(e.endsAt) : start + 50;
                      return (
                        <div
                          key={`${e.uid}-${e.recurrenceId ?? ""}`}
                          title={`${e.summary} · from your calendar`}
                          className="absolute left-[3px] right-[3px] overflow-hidden rounded-[6px] border border-dashed px-1.5 py-1"
                          style={{
                            top: (start - DAY_START) * PX_PER_MIN,
                            height: Math.max(16, (end - start) * PX_PER_MIN - 2),
                            borderColor: "var(--line)",
                            background: "var(--surface-hi)",
                          }}
                        >
                          <div className="truncate text-[10.5px] text-[var(--ink-dim)]">
                            {e.summary}
                          </div>
                        </div>
                      );
                    })}

                    {/* Your own blocks. */}
                    {dayBlocks.map((b) => {
                      const tint = tintOf(b.kind);
                      return (
                        <button
                          key={b.id}
                          onClick={(ev) => {
                            ev.stopPropagation();
                            setEditing({ block: b, weekday });
                          }}
                          title={`${b.title} · ${clock(b.startMin)}–${clock(b.endMin)}`}
                          className="pressable absolute left-[3px] right-[3px] overflow-hidden rounded-[6px] px-1.5 py-1 text-left"
                          style={{
                            top: (b.startMin - DAY_START) * PX_PER_MIN,
                            height: Math.max(18, (b.endMin - b.startMin) * PX_PER_MIN - 2),
                            background: `hsl(${tint} / ${b.available ? 0.1 : 0.17})`,
                            boxShadow: `inset 0 0 0 1px hsl(${tint} / 0.32)`,
                          }}
                        >
                          <div
                            className="truncate text-[10.5px] font-medium"
                            style={{ color: `hsl(${tint})` }}
                          >
                            {b.title}
                          </div>
                          {b.endMin - b.startMin >= 45 && (
                            <div className="truncate text-[9.5px] text-[var(--ink-faint)]">
                              {clock(b.startMin)}
                            </div>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </Card>

      <p className="mt-3 px-1 text-[12px] leading-relaxed text-[var(--ink-faint)]">
        Click anywhere in a column to add a block. Dashed blocks come from your Compass calendar
        and can't be edited here. Free hours under each day account for overlaps.
      </p>

      <section className="animate-rise mt-8">
        <SectionHeader title="What the assistant sees" />
        <Card className="p-5">
          <p className="text-[13px] leading-relaxed text-[var(--ink-dim)]">
            Your committed time is included whenever you ask the assistant what to work on, so it
            won't suggest a two-hour session on an evening you're at tuition. Blocks marked
            <span className="text-[var(--ink)]"> "I can study here"</span> aren't counted as
            committed.
          </p>
        </Card>
      </section>

      {editing && (
        <BlockEditor
          initial={editing.block}
          subjects={subjects}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await load();
          }}
        />
      )}
    </div>
  );
}

/** Local minutes-from-midnight for an instant. */
function minutesOf(iso: string): number {
  const d = new Date(iso);
  return d.getHours() * 60 + d.getMinutes();
}

function BlockEditor({
  initial,
  subjects,
  onClose,
  onSaved,
}: {
  initial: TimeBlock | null;
  subjects: ReturnType<typeof useApp.getState>["subjects"];
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const existing = initial && initial.id > 0;

  const [title, setTitle] = useState(initial?.title ?? "");
  const [kind, setKind] = useState<BlockKind>(initial?.kind ?? "other");
  const [startMin, setStartMin] = useState(initial?.startMin ?? 16 * 60);
  const [endMin, setEndMin] = useState(initial?.endMin ?? 17 * 60);
  const [available, setAvailable] = useState(initial?.available ?? false);
  const [subjectId, setSubjectId] = useState<number | null>(initial?.subjectId ?? null);
  const [repeats, setRepeats] = useState(initial?.onDate == null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const save = async () => {
    setBusy(true);
    setError(null);
    const payload: NewBlock = {
      title,
      kind,
      weekday: repeats ? (initial?.weekday ?? 0) : null,
      onDate: repeats ? null : (initial?.onDate ?? null),
      startMin,
      endMin,
      available,
      subjectId,
      note: null,
    };

    try {
      if (existing) await api.updateBlock(initial!.id, payload);
      else await api.createBlock(payload);
      await onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="scrim fixed inset-0 z-50 flex items-center justify-center px-8" onClick={onClose}>
      <div className="sheet animate-pop w-full max-w-[440px] p-7" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-start gap-3">
          <h2 className="flex-1 text-[19px] font-semibold tracking-[-0.02em]">
            {existing ? "Edit block" : "Block out some time"}
          </h2>
          <button
            onClick={onClose}
            aria-label="Close"
            className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            <X size={15} />
          </button>
        </div>

        <input
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Tuition, shift, dinner…"
          className="mt-5 h-10 w-full rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[14px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
        />

        <div className="mt-3 flex flex-wrap gap-1.5">
          {KINDS.map((k) => (
            <button
              key={k.value}
              onClick={() => setKind(k.value)}
              className={cx(
                "pressable rounded-full border px-2.5 py-1 text-[12px]",
                kind === k.value
                  ? "text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)]",
              )}
              style={
                kind === k.value
                  ? {
                      borderColor: `hsl(${k.tint} / 0.4)`,
                      background: `hsl(${k.tint} / 0.14)`,
                    }
                  : undefined
              }
            >
              {k.label}
            </button>
          ))}
        </div>

        <div className="mt-4 flex items-center gap-2">
          <TimeField label="From" value={startMin} onChange={setStartMin} />
          <TimeField label="To" value={endMin} onChange={setEndMin} />
        </div>

        <label className="mt-4 flex cursor-pointer items-start gap-2.5">
          <input
            type="checkbox"
            checked={available}
            onChange={(e) => setAvailable(e.target.checked)}
            className="mt-[3px]"
          />
          <span className="text-[13px] leading-relaxed">
            I can study here
            <span className="block text-[12px] text-[var(--ink-faint)]">
              A free period you actually work in, say. Unticked means Retain treats this time as
              gone.
            </span>
          </span>
        </label>

        <label className="mt-3 flex cursor-pointer items-center gap-2.5">
          <input
            type="checkbox"
            checked={repeats}
            onChange={(e) => setRepeats(e.target.checked)}
          />
          <span className="text-[13px]">Every week</span>
        </label>

        {subjects.length > 0 && (
          <select
            value={subjectId ?? ""}
            onChange={(e) => setSubjectId(e.target.value ? Number(e.target.value) : null)}
            className="mt-4 h-9 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[12.5px] text-[var(--ink)]"
          >
            <option value="">No subject</option>
            {subjects.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}

        {error && <p className="mt-3 text-[12.5px] text-[var(--danger)]">{error}</p>}

        <div className="mt-6 flex items-center gap-2">
          {existing && (
            <button
              onClick={async () => {
                await api.deleteBlock(initial!.id);
                await onSaved();
              }}
              className="pressable flex items-center gap-1.5 rounded-[var(--r-sm)] px-2 py-1.5 text-[12.5px] text-[var(--ink-faint)] hover:bg-[color-mix(in_srgb,var(--danger)_9%,transparent)] hover:text-[var(--danger)]"
            >
              <Trash2 size={13} />
              Remove
            </button>
          )}
          <Button
            variant="primary"
            className="ml-auto"
            disabled={busy || !title.trim()}
            onClick={() => void save()}
          >
            <Plus size={14} />
            {existing ? "Save" : "Add block"}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** A time picker in half-hour steps — finer than that is false precision here. */
function TimeField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
}) {
  const options = useMemo(() => {
    const out: number[] = [];
    for (let m = 0; m <= 1440; m += 30) out.push(m);
    return out;
  }, []);

  return (
    <label className="flex flex-1 items-center gap-2">
      <span className="text-[12.5px] text-[var(--ink-faint)]">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="tabular h-9 flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
      >
        {options.map((m) => (
          <option key={m} value={m}>
            {m === 1440 ? "midnight" : clock(m)}
          </option>
        ))}
      </select>
    </label>
  );
}

import { useEffect, useRef, useState } from "react";
import {
  Bell,
  BookOpen,
  CalendarDays,
  Database,
  Info,
  Palette,
  Plus,
  Sparkles,
  Timer,
  Trash2,
} from "lucide-react";

import { ApiKeyField } from "../components/ApiKeyField";
import { useAi } from "../components/Ai";
import { CalendarSection } from "../components/CalendarSection";
import { UpdateSection } from "../components/UpdateSection";

import { api } from "../lib/api";
import {
  PALETTE,
  SUBJECT_TYPE_LABELS,
  UNIT_LEVEL_LABELS,
  inferType,
  nextColour,
} from "../lib/catalogue";
import { WEEKDAY_SHORT } from "../lib/format";
import type {
  ModelOption,
  NotificationCandidate,
  NotificationSettings,
  Provider,
  Subject,
  SubjectType,
  UnitLevel,
} from "../lib/types";
import {
  Button,
  Card,
  ColourDot,
  Segmented,
  SectionTitle,
  Toggle,
  cx,
} from "../components/ui";
import { useApp } from "../store";

const PROVIDERS: { value: Provider; label: string }[] = [
  { value: "anthropic", label: "Anthropic" },
  { value: "open_ai", label: "OpenAI" },
  { value: "gemini", label: "Gemini" },
  { value: "open_router", label: "OpenRouter" },
];

/**
 * Settings.
 *
 * This was one column of eight stacked sections, about four screens tall. The
 * problem with that isn't ugliness, it's that you cannot answer "where is the
 * notification setting" without scrolling and reading — a settings page you
 * have to search through is one you avoid, so the things in it never get
 * changed.
 *
 * Two panes now: the sections on the left, one of them on the right. Same
 * sections, same components; only the shell changed. It's how macOS System
 * Settings works and it's right for the same reason — the list of what's
 * configurable is itself the most useful thing on the page.
 */

const SECTIONS = [
  {
    id: "subjects",
    label: "Subjects",
    Icon: BookOpen,
    blurb: "What you study",
  },
  {
    id: "studying",
    label: "Studying",
    Icon: Timer,
    blurb: "Sessions and rest days",
  },
  {
    id: "notifications",
    label: "Notifications",
    Icon: Bell,
    blurb: "When Retain speaks up",
  },
  { id: "ai", label: "AI", Icon: Sparkles, blurb: "Provider, model and key" },
  {
    id: "calendar",
    label: "Calendar",
    Icon: CalendarDays,
    blurb: "Your Compass feed",
  },
  { id: "appearance", label: "Appearance", Icon: Palette, blurb: "Theme" },
  {
    id: "data",
    label: "Data",
    Icon: Database,
    blurb: "Export, import, where it lives",
  },
  { id: "about", label: "About", Icon: Info, blurb: "Version and updates" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

export function Settings() {
  const {
    boot,
    subjects,
    streak,
    refreshSubjects,
    refreshProgress,
    setTheme,
    init,
  } = useApp();
  const [open, setOpen] = useState<SectionId>("subjects");

  const onSubjectsChange = async () => {
    await refreshSubjects();
    await refreshProgress();
  };

  return (
    <div className="mx-auto w-full max-w-[min(1080px,100%)] px-6 pb-16 sm:px-9">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-6">
        <h1 className="text-[28px] font-semibold tracking-[-0.028em]">
          Settings
        </h1>
      </header>

      <div className="flex flex-col gap-7 md:flex-row md:gap-8">
        {/* The list of what's configurable, which is itself the useful part. */}
        <nav className="shrink-0 md:w-[212px]">
          <div className="flex gap-1.5 overflow-x-auto pb-1 md:flex-col md:overflow-visible md:pb-0">
            {SECTIONS.map((s) => (
              <button
                key={s.id}
                onClick={() => setOpen(s.id)}
                aria-current={open === s.id}
                className={cx(
                  "flex shrink-0 items-center gap-2.5 rounded-[var(--r-md)] px-3 py-2 text-left transition-colors duration-[var(--t-fast)] md:w-full",
                  open === s.id
                    ? "bg-[var(--surface-hi)] text-[var(--ink)]"
                    : "text-[var(--ink-dim)] hover:bg-[var(--surface)] hover:text-[var(--ink)]",
                )}
              >
                <s.Icon
                  size={14}
                  className={cx(
                    "shrink-0",
                    open === s.id
                      ? "text-[var(--accent)]"
                      : "text-[var(--ink-faint)]",
                  )}
                />
                <span className="min-w-0">
                  <span className="block truncate text-[13.5px]">
                    {s.label}
                  </span>
                  {/* Hidden on the horizontal layout, where there's no room and
                      the label alone is enough. */}
                  <span className="hidden truncate text-[11.5px] text-[var(--ink-faint)] md:block">
                    {s.blurb}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </nav>

        <main className="animate-rise min-w-0 flex-1">
          {open === "subjects" && (
            <SubjectsSection subjects={subjects} onChange={onSubjectsChange} />
          )}

          {open === "studying" && (
            <StudyingSection
              threshold={
                streak?.thresholdMinutes ?? boot?.focusedSessionMinutes ?? 20
              }
              restDays={streak?.restDays ?? []}
              onChange={async () => {
                await init();
                await refreshProgress();
              }}
            />
          )}

          {open === "notifications" && <NotificationsSection />}
          {open === "ai" && (
            <div className="space-y-7">
              {/* The key comes first: every AI feature below is inert without
                  one, and a list of features you can't use reads as broken. */}
              <KeysSection />
              <AiSection />
            </div>
          )}
          {open === "calendar" && <CalendarSection />}

          {open === "appearance" && (
            <section>
              <SectionTitle>Appearance</SectionTitle>
              <Card className="mt-2.5 p-5">
                <div className="flex items-center justify-between gap-4">
                  <div className="min-w-0">
                    <div className="text-[14px] text-[var(--ink)]">Theme</div>
                    <p className="mt-0.5 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
                      Dark by default. Sticky notes stay on paper colours either
                      way — they sit on your wallpaper, not inside the app.
                    </p>
                  </div>
                  <Segmented
                    value={boot?.theme === "light" ? "light" : "dark"}
                    onChange={(v) => void setTheme(v as "dark" | "light")}
                    options={[
                      { value: "dark", label: "Dark" },
                      { value: "light", label: "Light" },
                    ]}
                  />
                </div>
              </Card>
            </section>
          )}

          {open === "data" && <DataSection onImported={init} />}

          {open === "about" && (
            <div className="space-y-7">
              <UpdateSection version={boot?.appVersion} />
              <section>
                <SectionTitle>About</SectionTitle>
                <Card className="mt-2.5 p-5 text-[13px] leading-relaxed text-[var(--ink-dim)]">
                  <div>Retain {boot?.appVersion}</div>
                  <div className="mt-1.5 text-[var(--ink-faint)]">
                    No account, no server, no telemetry. Everything is on this
                    Mac.
                  </div>
                </Card>
              </section>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

function SubjectsSection({
  subjects,
  onChange,
}: {
  subjects: Subject[];
  onChange: () => Promise<void>;
}) {
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const add = async () => {
    const name = newName.trim();
    if (!name) return;
    setError(null);
    try {
      await api.createSubject({
        name,
        colour: nextColour(subjects.map((s) => s.colour)),
        unitLevel: "1_2",
        subjectType: inferType(name),
        weeklyGoalMinutes: null,
      });
      setNewName("");
      setAdding(false);
      await onChange();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section>
      <SectionTitle>Subjects</SectionTitle>
      <Card className="mt-2.5 divide-y divide-[var(--line-soft)] overflow-hidden">
        {subjects.map((s) => (
          <SubjectRow key={s.id} subject={s} onChange={onChange} />
        ))}

        <div className="p-4">
          {adding ? (
            <div className="flex gap-2">
              <input
                autoFocus
                value={newName}
                placeholder="Subject name"
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void add();
                  if (e.key === "Escape") setAdding(false);
                }}
                className="h-8 flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13px] text-[var(--ink)]"
              />
              <Button size="sm" onClick={add}>
                Add
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setAdding(false)}
              >
                Cancel
              </Button>
            </div>
          ) : (
            <Button size="sm" variant="ghost" onClick={() => setAdding(true)}>
              <Plus size={14} />
              Add subject
            </Button>
          )}
          {error && (
            <div className="mt-2 text-[12.5px] text-[var(--danger)]">
              {error}
            </div>
          )}
        </div>
      </Card>
    </section>
  );
}

function SubjectRow({
  subject,
  onChange,
}: {
  subject: Subject;
  onChange: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(subject);
  const [goalHours, setGoalHours] = useState(
    subject.weeklyGoalMinutes ? String(subject.weeklyGoalMinutes / 60) : "",
  );

  const save = async (patch: Partial<Subject>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    await api.updateSubject(subject.id, {
      name: next.name,
      colour: next.colour,
      unitLevel: next.unitLevel,
      subjectType: next.subjectType,
      weeklyGoalMinutes: next.weeklyGoalMinutes,
    });
    await onChange();
  };

  const saveGoal = async (raw: string) => {
    setGoalHours(raw);
    const hours = Number(raw);
    const minutes =
      raw.trim() === "" || Number.isNaN(hours) ? null : Math.round(hours * 60);
    await api.setWeeklyGoal(subject.id, minutes);
    setDraft({ ...draft, weeklyGoalMinutes: minutes });
    await onChange();
  };

  return (
    <div className="px-5 py-3.5">
      <button
        className="flex w-full items-center gap-3 text-left"
        onClick={() => setOpen(!open)}
      >
        <ColourDot colour={draft.colour} size={9} />
        <span className="flex-1 text-[14px]">{draft.name}</span>
        <span className="text-[12px] text-[var(--ink-faint)]">
          {UNIT_LEVEL_LABELS[draft.unitLevel]} ·{" "}
          {SUBJECT_TYPE_LABELS[draft.subjectType]}
        </span>
      </button>

      {open && (
        <div className="animate-in mt-4 space-y-3.5">
          <input
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            onBlur={() => void save({})}
            className="h-8 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13px] text-[var(--ink)]"
          />

          <div className="flex flex-wrap gap-2">
            <Segmented
              size="sm"
              value={draft.unitLevel}
              onChange={(v: UnitLevel) => void save({ unitLevel: v })}
              options={[
                { value: "1_2", label: UNIT_LEVEL_LABELS["1_2"] },
                { value: "3_4", label: UNIT_LEVEL_LABELS["3_4"] },
              ]}
            />
            <Segmented
              size="sm"
              value={draft.subjectType}
              onChange={(v: SubjectType) => void save({ subjectType: v })}
              options={(Object.keys(SUBJECT_TYPE_LABELS) as SubjectType[]).map(
                (t) => ({
                  value: t,
                  label: SUBJECT_TYPE_LABELS[t],
                }),
              )}
            />
          </div>

          <div className="flex items-center gap-1.5">
            {PALETTE.map((c) => (
              <button
                key={c}
                aria-label={`Colour ${c}`}
                onClick={() => void save({ colour: c })}
                className={cx(
                  "h-5 w-5 rounded-full transition-all duration-[120ms]",
                  draft.colour === c
                    ? "ring-2 ring-[var(--ink-dim)] ring-offset-2 ring-offset-[var(--surface)]"
                    : "opacity-55 hover:opacity-100",
                )}
                style={{ background: c }}
              />
            ))}
          </div>

          <div className="flex items-center gap-2">
            <span className="text-[13px] text-[var(--ink-dim)]">
              Weekly goal
            </span>
            <input
              type="number"
              min={0}
              max={40}
              step={0.5}
              value={goalHours}
              placeholder="—"
              onChange={(e) => void saveGoal(e.target.value)}
              className="tabular h-7 w-[64px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[12.5px] text-[var(--ink)]"
            />
            <span className="text-[13px] text-[var(--ink-faint)]">hours</span>
          </div>

          <button
            onClick={async () => {
              await api.archiveSubject(subject.id);
              await onChange();
            }}
            className="flex items-center gap-1.5 text-[12.5px] text-[var(--ink-faint)] transition-colors hover:text-[var(--danger)]"
          >
            <Trash2 size={13} />
            Remove from active subjects
          </button>
          <p className="text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
            Removing a subject hides it from pickers and rings. Its sessions and
            history stay exactly as they are.
          </p>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------

function StudyingSection({
  threshold,
  restDays,
  onChange,
}: {
  threshold: number;
  restDays: number[];
  onChange: () => Promise<void>;
}) {
  const [value, setValue] = useState(threshold);
  useEffect(() => setValue(threshold), [threshold]);

  const toggleRest = async (day: number) => {
    const next = restDays.includes(day)
      ? restDays.filter((d) => d !== day)
      : [...restDays, day];
    await api.setRestDays(next);
    await onChange();
  };

  return (
    <section>
      <SectionTitle>Studying</SectionTitle>
      <Card className="mt-2.5 p-5">
        <div className="flex items-center justify-between gap-6">
          <div>
            <div className="text-[14px]">Focused session length</div>
            <div className="mt-0.5 max-w-[400px] text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
              How much active time one session needs to earn the day. Pauses,
              idle time and breaks don't count toward it. Defaults to 20, a
              little under a typical study block so a real block that included
              some idle time still counts.
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <input
              type="number"
              min={5}
              max={120}
              value={value}
              onChange={(e) => setValue(Number(e.target.value))}
              onBlur={async () => {
                await api.setSetting("focused_session_minutes", String(value));
                await onChange();
              }}
              className="tabular h-8 w-[58px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[13px] text-[var(--ink)]"
            />
            <span className="text-[13px] text-[var(--ink-faint)]">min</span>
          </div>
        </div>

        <div className="mt-5 border-t border-[var(--line-soft)] pt-5">
          <div className="text-[14px]">Rest days</div>
          <div className="mt-0.5 text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
            Days off never break a run and never use a freeze.
          </div>
          <div className="mt-3 flex gap-1.5">
            {WEEKDAY_SHORT.map((label, i) => (
              <button
                key={label}
                onClick={() => void toggleRest(i)}
                className={cx(
                  "h-8 w-[46px] rounded-[var(--r-sm)] border text-[12.5px] transition-all duration-[120ms]",
                  restDays.includes(i)
                    ? "border-[var(--ink-faint)] bg-[var(--surface-hi)] text-[var(--ink)]"
                    : "border-[var(--line)] text-[var(--ink-faint)] hover:border-[var(--ink-faint)]",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </Card>
    </section>
  );
}

// ---------------------------------------------------------------------------

/**
 * Notification controls.
 *
 * The limitation is stated in the interface rather than left to be discovered:
 * these are state-triggered and evaluated by the running app, so a full quit
 * means nothing fires until Retain is open again.
 */
function NotificationsSection() {
  const [s, setS] = useState<NotificationSettings | null>(null);
  const [sent, setSent] = useState(0);
  const [preview, setPreview] = useState<NotificationCandidate[]>([]);

  const load = async () => {
    const [settings, count, pending] = await Promise.all([
      api.notificationSettings(),
      api.notificationsSentToday(),
      api.previewNotifications(),
    ]);
    setS(settings);
    setSent(count);
    setPreview(pending);
  };

  useEffect(() => {
    void load();
  }, []);

  const save = async (next: NotificationSettings) => {
    setS(next);
    await api.setNotificationSettings(next);
  };

  if (!s) return null;

  const CATS: {
    key: keyof NotificationSettings;
    label: string;
    blurb: string;
  }[] = [
    {
      key: "reviews",
      label: "Reviews due",
      blurb: "When cards are actually waiting.",
    },
    {
      key: "assessments",
      label: "Assessments",
      blurb: "Weekly far out, daily in the last fortnight.",
    },
    {
      key: "topicDecay",
      label: "Topic decay",
      blurb: "A shaky topic you haven't touched.",
    },
    {
      key: "streak",
      label: "Streak",
      blurb: "Only while today is still winnable.",
    },
  ];

  return (
    <section>
      <SectionTitle>Notifications</SectionTitle>
      <Card className="mt-2.5 p-5">
        <Toggle
          checked={s.enabled}
          onChange={(v) => void save({ ...s, enabled: v })}
          label="Send notifications"
          description="State-triggered only — they fire because something changed, never on a schedule."
        />

        {s.enabled && (
          <>
            <div className="mt-4 flex items-center justify-between border-t border-[var(--line-soft)] pt-4">
              <div>
                <div className="text-[14px]">Quiet hours</div>
                <div className="mt-0.5 text-[12.5px] text-[var(--ink-faint)]">
                  Nothing arrives between these times.
                </div>
              </div>
              <div className="flex items-center gap-1.5 text-[13px] text-[var(--ink-faint)]">
                <select
                  value={s.quietFromHour}
                  onChange={(e) =>
                    void save({ ...s, quietFromHour: Number(e.target.value) })
                  }
                  className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
                >
                  {Array.from({ length: 24 }, (_, h) => (
                    <option key={h} value={h}>
                      {String(h).padStart(2, "0")}:00
                    </option>
                  ))}
                </select>
                <span>to</span>
                <select
                  value={s.quietToHour}
                  onChange={(e) =>
                    void save({ ...s, quietToHour: Number(e.target.value) })
                  }
                  className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
                >
                  {Array.from({ length: 24 }, (_, h) => (
                    <option key={h} value={h}>
                      {String(h).padStart(2, "0")}:00
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className="mt-4 flex items-center justify-between border-t border-[var(--line-soft)] pt-4">
              <div>
                <div className="text-[14px]">Most per day</div>
                <div className="mt-0.5 text-[12.5px] text-[var(--ink-faint)]">
                  {sent} sent today.
                </div>
              </div>
              <input
                type="number"
                min={0}
                max={20}
                value={s.dailyCap}
                onChange={(e) =>
                  void save({ ...s, dailyCap: Number(e.target.value) })
                }
                className="tabular h-8 w-[58px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[13px] text-[var(--ink)]"
              />
            </div>

            <div className="mt-2 border-t border-[var(--line-soft)] pt-2">
              {CATS.map((c) => (
                <Toggle
                  key={c.key}
                  checked={s[c.key] as boolean}
                  onChange={(v) => void save({ ...s, [c.key]: v })}
                  label={c.label}
                  description={c.blurb}
                />
              ))}
            </div>

            {preview.length > 0 && (
              <div className="mt-4 rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface-hi)] p-3">
                <div className="text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">
                  What you'd receive right now
                </div>
                <div className="mt-2 space-y-2">
                  {preview.map((p) => (
                    <div
                      key={p.dedupeKey}
                      className="text-[12.5px] leading-relaxed"
                    >
                      <span className="text-[var(--ink)]">{p.title}</span>
                      <div className="text-[var(--ink-faint)]">{p.body}</div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}

        <p className="mt-4 border-t border-[var(--line-soft)] pt-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          Retain evaluates these while it's running. Closing the window is fine
          — it keeps going in the menu bar. But if you quit entirely (⌘Q),
          nothing fires until you open it again. There is no background helper,
          and adding one would mean a second always-on process with its own
          permissions and updates.
        </p>
      </Card>
    </section>
  );
}

function KeysSection() {
  const [present, setPresent] = useState<Record<string, boolean>>({});
  const refreshAi = useApp((s) => s.refreshAi);

  const refresh = async () => {
    const entries = await Promise.all(
      PROVIDERS.map(
        async (p) => [p.value, await api.secretHas(p.value)] as const,
      ),
    );
    setPresent(Object.fromEntries(entries));
    // Adding or removing a key changes what the AI features offer, and that
    // status is cached in the store — refresh it here or the section below
    // stays stale until the next launch.
    await refreshAi();
  };

  useEffect(() => {
    void refresh();
  }, []);

  return (
    <section>
      <SectionTitle>AI keys</SectionTitle>
      <Card className="mt-2.5 divide-y divide-[var(--line-soft)] overflow-hidden">
        {PROVIDERS.map((p) => (
          <ApiKeyField
            key={p.value}
            provider={p.value}
            label={p.label}
            present={!!present[p.value]}
            onChange={refresh}
          />
        ))}

        <p className="px-5 py-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          Keys are checked with the provider before they're saved, so a typo or
          a half-copied key is caught here rather than weeks later. Checking
          uses each provider's model-list endpoint — it costs nothing and
          consumes no tokens.
          <br />
          <br />
          Keys live in your macOS Keychain and nowhere else — not in Retain's
          database, its settings, its exports, or any log. Retain can check that
          a key exists but has no way to read one back out to this screen.
          Because the app is ad-hoc signed rather than signed with a paid
          Developer ID, its code signature changes with every build — so macOS
          will ask you to allow Keychain access again after each update.
        </p>
      </Card>
    </section>
  );
}

// ---------------------------------------------------------------------------

function DataSection({ onImported }: { onImported: () => Promise<void> }) {
  const [message, setMessage] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const doExport = async () => {
    try {
      setMessage(`Saved to ${await api.exportToFile()}`);
    } catch (e) {
      setMessage(String(e));
    }
  };

  const doImport = async (file: File) => {
    try {
      const report = await api.importJson(await file.text());
      setMessage(
        `Restored ${report.rowsWritten} rows across ${report.tablesWritten} tables.`,
      );
      await onImported();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setConfirming(false);
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  return (
    <section>
      <SectionTitle>Data</SectionTitle>
      <Card className="mt-2.5 p-5">
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={doExport}>
            Export everything
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setConfirming(true)}>
            Restore from export
          </Button>
        </div>

        <input
          ref={fileRef}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void doImport(f);
          }}
        />

        {confirming && (
          <div className="animate-in mt-4 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--warn)_35%,transparent)] bg-[color-mix(in_srgb,var(--warn)_10%,transparent)] p-4">
            <div className="text-[13px] leading-relaxed text-[var(--ink)]">
              Restoring replaces everything currently in Retain with the
              contents of the file. A snapshot of your current data is taken
              first, so this is reversible.
            </div>
            <div className="mt-3 flex gap-2">
              <Button size="sm" onClick={() => fileRef.current?.click()}>
                Choose file
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setConfirming(false)}
              >
                Cancel
              </Button>
            </div>
          </div>
        )}

        {message && (
          <div className="selectable mt-3 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
            {message}
          </div>
        )}

        <p className="mt-4 border-t border-[var(--line-soft)] pt-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          The export is every table, in plain JSON — it stays complete as Retain
          grows. Your database also gets snapshotted automatically on each
          launch, keeping the last seven.
          <br />
          <br />
          Keeping the live database in iCloud Drive isn't offered: a SQLite file
          on a file-by-file syncing service corrupts, and the reasons are
          written up in
          <span className="selectable"> docs/icloud-sqlite-analysis.md</span>.
          Exports are the safe way to move data between Macs.
        </p>
      </Card>
    </section>
  );
}

// ---------------------------------------------------------------------------

/**
 * Which provider the AI features use, and which model.
 *
 * The model name is a free-text field on purpose. Providers retire model names
 * on their own schedule, and a hard-coded dropdown in a desktop app that ships
 * a few times a year would strand the user on a dead model with no way out. If
 * a call starts failing with "doesn't recognise that model", this is the field
 * to change — no update required.
 */
function AiSection() {
  const { status, reload } = useAi();
  const [model, setModel] = useState("");
  const [models, setModels] = useState<ModelOption[]>([]);
  const [discovering, setDiscovering] = useState(false);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(
    null,
  );

  useEffect(() => {
    if (status) setModel(status.model);
    setResult(null);
    setModels([]);
  }, [status?.provider, status?.model]);

  if (!status) return null;

  if (status.available.length === 0) {
    return (
      <section>
        <SectionTitle>AI features</SectionTitle>
        <Card className="mt-2.5 p-5">
          <p className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
            Retain has five optional AI features: structuring a captured note
            into a task, generating flashcards from pasted notes, writing your
            weekly review, drafting VCAA-style practice questions, and
            suggesting a category for a logged mistake.
            <br />
            <br />
            Add a key above to turn them on. Nothing else in Retain uses AI —
            the timer, cards, error log, streak and everything else work exactly
            the same with no key at all.
          </p>
        </Card>
      </section>
    );
  }

  const provider = status.provider;
  const dirty = model.trim() !== status.model;
  const usable = models.filter((m) => m.supportsGenerateContent);

  const save = async (value: string) => {
    if (!provider) return;
    await api.aiSetModel(provider, value.trim());
    setResult(null);
    reload();
  };

  return (
    <section>
      <SectionTitle>AI features</SectionTitle>
      <Card className="mt-2.5 p-5 space-y-4">
        {status.available.length > 1 && (
          <div className="flex items-center justify-between gap-4">
            <span className="text-[14px]">Use</span>
            <Segmented
              size="sm"
              value={provider ?? status.available[0]}
              onChange={async (v) => {
                await api.aiSetProvider(v as Provider);
                reload();
              }}
              options={status.available.map((p) => ({
                value: p,
                label: PROVIDERS.find((x) => x.value === p)?.label ?? p,
              }))}
            />
          </div>
        )}

        <div>
          <div className="flex items-baseline justify-between gap-3">
            <label className="text-[14px]">Model</label>
            {/* The model actually in use, always visible — the previous version
                made you guess what was being sent. */}
            <span className="truncate font-mono text-[11.5px] text-[var(--ink-faint)]">
              in use: {status.model}
            </span>
          </div>

          <div className="mt-2 flex items-center gap-2">
            <input
              value={model}
              onChange={(e) => {
                setModel(e.target.value);
                setResult(null);
              }}
              spellCheck={false}
              list="ai-model-options"
              className="h-9 flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 font-mono text-[12.5px] text-[var(--ink)] outline-none focus:border-[var(--ink-faint)]"
            />
            {/* Discovered models become autocomplete suggestions rather than a
                hard dropdown: a model can be missing from the list and still
                work, so the field must stay free text. */}
            <datalist id="ai-model-options">
              {usable.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.displayName}
                </option>
              ))}
            </datalist>

            <Button
              size="sm"
              disabled={!dirty}
              onClick={() => void save(model)}
            >
              Save
            </Button>
          </div>

          <div className="mt-2.5 flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              disabled={testing || !provider}
              onClick={async () => {
                setTesting(true);
                setResult(null);
                try {
                  setResult({
                    ok: true,
                    message: await api.testAiModel(provider!, model),
                  });
                } catch (e) {
                  setResult({ ok: false, message: String(e) });
                } finally {
                  setTesting(false);
                }
              }}
            >
              {testing ? "Testing…" : "Test this model"}
            </Button>

            {provider === "gemini" && (
              <Button
                size="sm"
                variant="ghost"
                disabled={discovering}
                onClick={async () => {
                  setDiscovering(true);
                  setResult(null);
                  try {
                    setModels(await api.listAiModels("gemini"));
                  } catch (e) {
                    setResult({ ok: false, message: String(e) });
                  } finally {
                    setDiscovering(false);
                  }
                }}
              >
                {discovering ? "Asking Gemini…" : "Find available models"}
              </Button>
            )}
          </div>

          {result && (
            <p
              className={cx(
                "mt-2.5 text-[12.5px] leading-relaxed",
                result.ok
                  ? "text-[var(--color-positive)]"
                  : "text-[var(--danger)]",
              )}
            >
              {result.ok ? "✓ " : ""}
              {result.message}
            </p>
          )}

          {usable.length > 0 && (
            <div className="mt-3">
              <div className="text-[11.5px] text-[var(--ink-faint)]">
                {usable.length} models support generation. Click one to use it:
              </div>
              <div className="mt-1.5 flex max-h-32 flex-wrap gap-1.5 overflow-y-auto">
                {usable.map((m) => (
                  <button
                    key={m.id}
                    onClick={() => {
                      setModel(m.id);
                      void save(m.id);
                    }}
                    className={cx(
                      "rounded-full border px-2.5 py-1 font-mono text-[11.5px] transition-colors",
                      m.id === status.model
                        ? "border-[var(--ink-faint)] text-[var(--ink)]"
                        : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)]",
                    )}
                  >
                    {m.id}
                  </button>
                ))}
              </div>
              <p className="mt-2 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
                Being listed doesn't guarantee it runs — some models are listed
                but still refuse the request. Use Test to be sure.
              </p>
            </div>
          )}

          <p className="mt-2.5 text-[12px] leading-relaxed text-[var(--ink-faint)]">
            Providers retire model names on their own schedule, which is why
            this is an editable field. Gemini's default is the maintained alias{" "}
            <code className="font-mono">gemini-flash-latest</code> rather than a
            pinned version, so it shouldn't go stale again.
          </p>
        </div>

        <p className="border-t border-[var(--line-soft)] pt-4 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          Every AI feature returns a suggestion you edit and confirm. Nothing is
          written to your cards, tasks or error log without you accepting it.
          Your weekly review's numbers are calculated by Retain, not by the
          model — only the wording around them is generated.
        </p>
      </Card>
    </section>
  );
}

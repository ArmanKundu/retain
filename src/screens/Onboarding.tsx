// First launch.
//
// Rewritten after the subject step turned out to be genuinely broken, not just
// busy. Two real bugs, both worth naming because they're easy to reintroduce:
//
//   1. `add` closed over `drafts` and wrote `setDrafts([...drafts, next])`.
//      Tapping two subject pills in quick succession meant both handlers saw
//      the same array and the second overwrote the first — a subject you'd
//      picked silently vanished. Every mutation here now goes through a
//      functional update, which is the only form that composes.
//
//   2. Rows were keyed on `name-index`. Removing a subject shifted every key
//      after it, so React reused the wrong DOM nodes and the segmented
//      controls appeared to jump to another subject's values.
//
// The shape changed too. Before, adding a subject immediately unfolded a card
// with a unit-level control, a four-way type control and six colour swatches —
// so choosing six subjects meant thirty-six decisions before you'd used the app
// once. Now picking is one tap, and the details are behind a disclosure that
// most people will never open, because the defaults are nearly always right.

import { useEffect, useMemo, useRef, useState } from "react";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { Bell, Check, ChevronLeft, GraduationCap, Plus, Sparkles, X } from "lucide-react";

import { api } from "../lib/api";
import {
  PALETTE,
  SUBJECT_TYPE_LABELS,
  UNIT_LEVEL_LABELS,
  VCE_SUBJECTS,
  inferType,
  nextColour,
} from "../lib/catalogue";
import type { Provider, SubjectInput, SubjectType, UnitLevel } from "../lib/types";
import { ApiKeyField } from "../components/ApiKeyField";
import { Button, ColourDot, Segmented, cx } from "../components/ui";
import { useApp } from "../store";

const MAX = 8;

/** A subject being assembled, before it exists in the database. */
interface Draft {
  /** Stable across reorders and removals — never the array index. */
  id: string;
  name: string;
  colour: string;
  unitLevel: UnitLevel;
  subjectType: SubjectType;
}

const STEPS = ["You", "Subjects", "Reminders", "Assistant"] as const;

export function Onboarding() {
  const [step, setStep] = useState(0);
  const [name, setName] = useState("");
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const init = useApp((s) => s.init);

  const finish = async () => {
    setSaving(true);
    setError(null);
    try {
      for (const d of drafts) {
        const input: SubjectInput = {
          name: d.name,
          colour: d.colour,
          unitLevel: d.unitLevel,
          subjectType: d.subjectType,
          weeklyGoalMinutes: null,
        };
        await api.createSubject(input);
      }
      await api.completeOnboarding(name);
      // Folders for each subject, so the Library is usable immediately rather
      // than after a trip to Settings.
      await api.ensureSubjectFolders().catch(() => {});
      await init();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const canContinue = step === 0 ? name.trim().length > 0 : step === 1 ? drafts.length > 0 : true;
  const last = step === STEPS.length - 1;

  return (
    <div className="app-wash flex h-full flex-col">
      <div className="titlebar-drag h-11 shrink-0" />

      {/* Progress. Segments rather than a bar: four steps is a countable
          number, and seeing "two to go" is more reassuring than a percentage. */}
      <div className="mx-auto flex w-full max-w-[560px] shrink-0 gap-1.5 px-8">
        {STEPS.map((label, i) => (
          <div key={label} className="flex-1">
            <div
              className={cx(
                "h-[3px] rounded-full transition-colors duration-300",
                i <= step ? "bg-[var(--accent)]" : "bg-[var(--line)]",
              )}
            />
            <div
              className={cx(
                "mt-1.5 text-[11px] transition-colors duration-300",
                i === step ? "text-[var(--ink)]" : "text-[var(--ink-faint)]",
              )}
            >
              {label}
            </div>
          </div>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[560px] px-8 py-8">
          {step === 0 && <NameStep name={name} setName={setName} onEnter={() => setStep(1)} />}
          {step === 1 && <SubjectsStep drafts={drafts} setDrafts={setDrafts} />}
          {step === 2 && <NotificationsStep />}
          {step === 3 && <AssistantStep />}
        </div>
      </div>

      <div className="shrink-0 border-t border-[var(--line-soft)] bg-[color-mix(in_srgb,var(--surface)_70%,transparent)]">
        <div className="mx-auto flex w-full max-w-[560px] items-center gap-3 px-8 py-4">
          {step > 0 && (
            <button
              onClick={() => setStep((s) => s - 1)}
              className="pressable flex items-center gap-1 rounded-[var(--r-sm)] px-2 py-1.5 text-[13px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
            >
              <ChevronLeft size={15} />
              Back
            </button>
          )}

          {error && <span className="text-[12.5px] text-[var(--danger)]">{error}</span>}

          <Button
            size="lg"
            variant="primary"
            className="ml-auto min-w-[140px]"
            disabled={!canContinue || saving}
            onClick={() => (last ? void finish() : setStep((s) => s + 1))}
          >
            {saving ? "Setting up…" : last ? "Start studying" : "Continue"}
          </Button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function Heading({ title, body }: { title: string; body?: string }) {
  return (
    <header className="animate-rise mb-7">
      <h1 className="text-[27px] font-semibold leading-tight tracking-[-0.028em]">{title}</h1>
      {body && (
        <p className="mt-2 text-[14.5px] leading-relaxed text-[var(--ink-dim)]">{body}</p>
      )}
    </header>
  );
}

function NameStep({
  name,
  setName,
  onEnter,
}: {
  name: string;
  setName: (v: string) => void;
  onEnter: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => ref.current?.focus(), []);

  return (
    <>
      <Heading
        title="Let's set you up."
        body="Retain lives entirely on this Mac. No account, no server, nothing sent anywhere."
      />
      <label className="block text-[13.5px] font-medium">What should I call you?</label>
      <input
        ref={ref}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && name.trim() && onEnter()}
        placeholder="Your name"
        className="mt-2 h-12 w-full rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] px-4 text-[16px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
      />
    </>
  );
}

// ---------------------------------------------------------------------------

/**
 * Picking subjects.
 *
 * One tap adds; one tap removes. Everything else — unit level, type, colour —
 * is behind a disclosure, because the defaults are right almost always and
 * thirty-six decisions before first use is how an app gets abandoned during
 * setup.
 */
function SubjectsStep({
  drafts,
  setDrafts,
}: {
  drafts: Draft[];
  setDrafts: React.Dispatch<React.SetStateAction<Draft[]>>;
}) {
  const [custom, setCustom] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  const chosen = useMemo(
    () => new Set(drafts.map((d) => d.name.toLowerCase())),
    [drafts],
  );

  /**
   * Add a subject.
   *
   * Functional update throughout. The previous version read `drafts` from the
   * closure, so two taps in the same tick both saw the same array and the
   * second silently dropped the first.
   */
  const add = (subjectName: string, type?: SubjectType) => {
    const clean = subjectName.trim();
    if (!clean) return;

    setDrafts((current) => {
      if (current.length >= MAX) return current;
      if (current.some((d) => d.name.toLowerCase() === clean.toLowerCase())) return current;

      return [
        ...current,
        {
          id: `${clean}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
          name: clean,
          // Derived from `current`, so two adds in one tick can't collide on a
          // colour either.
          colour: nextColour(current.map((d) => d.colour)),
          unitLevel: "1_2",
          subjectType: type ?? inferType(clean),
        },
      ];
    });
    setCustom("");
  };

  const remove = (id: string) => setDrafts((c) => c.filter((d) => d.id !== id));

  const patch = (id: string, changes: Partial<Draft>) =>
    setDrafts((c) => c.map((d) => (d.id === id ? { ...d, ...changes } : d)));

  const full = drafts.length >= MAX;

  return (
    <>
      <Heading
        title="Which subjects are you taking?"
        body="Tap to add. You can change any of this later — nothing here is permanent."
      />

      {/* The catalogue. Chosen subjects stay in place and turn filled rather
          than disappearing, so the list doesn't reflow under your finger
          mid-tap — which is what made the old one feel unreliable. */}
      <div className="flex flex-wrap gap-2">
        {VCE_SUBJECTS.map((s) => {
          const isChosen = chosen.has(s.name.toLowerCase());
          const draft = drafts.find((d) => d.name.toLowerCase() === s.name.toLowerCase());

          return (
            <button
              key={s.name}
              onClick={() => (isChosen && draft ? remove(draft.id) : add(s.name, s.type))}
              disabled={!isChosen && full}
              aria-pressed={isChosen}
              className={cx(
                "pressable flex items-center gap-2 rounded-full border px-3.5 py-2 text-[13.5px]",
                "disabled:opacity-35 disabled:pointer-events-none",
                isChosen
                  ? "text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)] hover:text-[var(--ink)]",
              )}
              style={
                isChosen && draft
                  ? {
                      borderColor: `color-mix(in srgb, ${draft.colour} 45%, transparent)`,
                      background: `color-mix(in srgb, ${draft.colour} 14%, transparent)`,
                    }
                  : undefined
              }
            >
              {isChosen && draft ? (
                <ColourDot colour={draft.colour} size={8} />
              ) : (
                <Plus size={13} className="opacity-50" />
              )}
              {s.name}
            </button>
          );
        })}
      </div>

      <div className="mt-4 flex gap-2">
        <input
          value={custom}
          placeholder="Something else…"
          disabled={full}
          onChange={(e) => setCustom(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add(custom)}
          className="h-10 flex-1 rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] px-3.5 text-[13.5px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)] disabled:opacity-40"
        />
        <Button onClick={() => add(custom)} disabled={!custom.trim() || full}>
          Add
        </Button>
      </div>

      {full && (
        <p className="mt-2 text-[12.5px] text-[var(--ink-faint)]">
          That's {MAX} — plenty to be going on with. Remove one to swap it out.
        </p>
      )}

      {drafts.length > 0 && (
        <div className="animate-rise mt-8">
          <div className="mb-2.5 flex items-baseline gap-2">
            <h2 className="text-[14px] font-medium">
              {drafts.length} {drafts.length === 1 ? "subject" : "subjects"}
            </h2>
            <span className="text-[12px] text-[var(--ink-faint)]">
              tap one to set its level
            </span>
          </div>

          <div className="space-y-1.5">
            {drafts.map((d) => (
              // Keyed on a stable id. Index keys made removal reuse the wrong
              // row and the controls appeared to jump between subjects.
              <div
                key={d.id}
                className="overflow-hidden rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface)]"
              >
                <div className="flex items-center gap-3 px-3.5 py-2.5">
                  <ColourDot colour={d.colour} size={10} />
                  <button
                    onClick={() => setExpanded(expanded === d.id ? null : d.id)}
                    className="min-w-0 flex-1 text-left"
                  >
                    <span className="block truncate text-[14px] text-[var(--ink)]">{d.name}</span>
                    <span className="block text-[11.5px] text-[var(--ink-faint)]">
                      {UNIT_LEVEL_LABELS[d.unitLevel]} · {SUBJECT_TYPE_LABELS[d.subjectType]}
                    </span>
                  </button>
                  <button
                    aria-label={`Remove ${d.name}`}
                    onClick={() => remove(d.id)}
                    className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:text-[var(--ink)]"
                  >
                    <X size={14} />
                  </button>
                </div>

                {expanded === d.id && (
                  <div className="animate-rise space-y-3 border-t border-[var(--line-soft)] px-3.5 py-3">
                    <Segmented
                      size="sm"
                      value={d.unitLevel}
                      onChange={(v) => patch(d.id, { unitLevel: v })}
                      options={[
                        { value: "1_2", label: UNIT_LEVEL_LABELS["1_2"] },
                        { value: "3_4", label: UNIT_LEVEL_LABELS["3_4"] },
                      ]}
                    />
                    <Segmented
                      size="sm"
                      value={d.subjectType}
                      onChange={(v) => patch(d.id, { subjectType: v })}
                      options={(Object.keys(SUBJECT_TYPE_LABELS) as SubjectType[]).map((t) => ({
                        value: t,
                        label: SUBJECT_TYPE_LABELS[t],
                      }))}
                    />
                    <div className="flex flex-wrap items-center gap-1.5">
                      {PALETTE.map((c) => (
                        <button
                          key={c}
                          aria-label={`Colour ${c}`}
                          onClick={() => patch(d.id, { colour: c })}
                          className={cx(
                            "pressable h-5 w-5 rounded-full",
                            d.colour === c && "ring-2 ring-[var(--ink)] ring-offset-2 ring-offset-[var(--surface)]",
                          )}
                          style={{ background: c }}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------

function NotificationsStep() {
  const [granted, setGranted] = useState<boolean | null>(null);
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    void isPermissionGranted().then(setGranted).catch(() => setGranted(false));
  }, []);

  return (
    <>
      <Heading
        title="Want a nudge when something's due?"
        body="Retain only notifies you because something changed — cards actually waiting, an assessment approaching. Never on a schedule, and never more than three a day."
      />

      <div className="rounded-[var(--r-lg)] border border-[var(--line-soft)] bg-[var(--surface)] p-5">
        <div className="flex items-start gap-3">
          <div
            className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full"
            style={{ background: "color-mix(in srgb, var(--accent) 14%, transparent)" }}
          >
            {granted ? (
              <Check size={16} className="text-[var(--color-positive)]" />
            ) : (
              <Bell size={16} className="text-[var(--accent)]" />
            )}
          </div>

          <div className="min-w-0 flex-1">
            <div className="text-[14px] font-medium">
              {granted ? "Notifications are on" : "Notifications are off"}
            </div>
            <p className="mt-1 text-[13px] leading-relaxed text-[var(--ink-dim)]">
              {granted
                ? "You can turn individual kinds off in Settings whenever you like."
                : "Entirely optional — everything works without them."}
            </p>

            {!granted && (
              <Button
                size="sm"
                className="mt-3"
                disabled={asking}
                onClick={async () => {
                  setAsking(true);
                  try {
                    setGranted((await requestPermission()) === "granted");
                  } finally {
                    setAsking(false);
                  }
                }}
              >
                {asking ? "Asking macOS…" : "Turn them on"}
              </Button>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------

function AssistantStep() {
  const [provider, setProvider] = useState<Provider>("anthropic");
  const [present, setPresent] = useState(false);

  const refresh = () =>
    void api
      .secretHas(provider)
      .then(setPresent)
      .catch(() => setPresent(false));

  useEffect(refresh, [provider]);

  return (
    <>
      <Heading
        title="One last, optional thing."
        body="With an API key, Retain can write notes from your own material, draft practice questions and answer from the documents you upload. Without one, everything else works exactly the same."
      />

      <div className="rounded-[var(--r-lg)] border border-[var(--line-soft)] bg-[var(--surface)] p-5">
        <div className="mb-4 flex items-center gap-2.5">
          <Sparkles size={15} className="text-[var(--accent)]" />
          <span className="text-[14px] font-medium">Add a key</span>
          <span className="ml-auto text-[12px] text-[var(--ink-faint)]">Skip if you'd rather</span>
        </div>

        <Segmented
          size="sm"
          value={provider}
          onChange={setProvider}
          options={[
            { value: "anthropic", label: "Anthropic" },
            { value: "open_ai", label: "OpenAI" },
            { value: "gemini", label: "Gemini" },
            { value: "open_router", label: "OpenRouter" },
          ]}
        />

        <div className="mt-3 rounded-[var(--r-md)] border border-[var(--line-soft)]">
          <ApiKeyField
            provider={provider}
            label={provider === "open_ai" ? "OpenAI" : provider === "open_router" ? "OpenRouter" : provider === "gemini" ? "Gemini" : "Anthropic"}
            present={present}
            onChange={async () => refresh()}
          />
        </div>

        <p className="mt-3 text-[12px] leading-relaxed text-[var(--ink-faint)]">
          Keys are checked with the provider before they're saved, and kept in your macOS Keychain
          — never in Retain's database, its exports, or any log.
        </p>
      </div>

      <div className="mt-6 flex items-start gap-2.5 text-[13px] leading-relaxed text-[var(--ink-dim)]">
        <GraduationCap size={16} className="mt-0.5 shrink-0 text-[var(--ink-faint)]" />
        <p>
          That's everything. Retain will make a folder for each subject in Documents — drop your
          notes and past papers in whenever you're ready.
        </p>
      </div>
    </>
  );
}

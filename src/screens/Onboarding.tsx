// First launch. Four screens: name, subjects, notifications, AI key.

import { useState } from "react";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { Check, ChevronLeft, Plus, X } from "lucide-react";

import { api } from "../lib/api";
import {
  PALETTE,
  SUBJECT_TYPE_BLURB,
  SUBJECT_TYPE_LABELS,
  UNIT_LEVEL_LABELS,
  VCE_SUBJECTS,
  inferType,
  nextColour,
} from "../lib/catalogue";
import type { Provider, SubjectInput, SubjectType, UnitLevel } from "../lib/types";
import { ApiKeyField } from "../components/ApiKeyField";
import { Button, Card, ColourDot, Segmented, TextField, cx } from "../components/ui";
import { useApp } from "../store";

const MAX = 6;

/** A subject being assembled in step 2, before it exists in the database. */
interface Draft {
  name: string;
  colour: string;
  unitLevel: UnitLevel;
  subjectType: SubjectType;
  /** Whether the user overrode the inferred type, so re-inference stops. */
  typeLocked: boolean;
}

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
      await init();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const steps = [
    <NameStep key="name" name={name} setName={setName} />,
    <SubjectsStep key="subjects" drafts={drafts} setDrafts={setDrafts} />,
    <NotificationsStep key="notify" />,
    <KeyStep key="key" />,
  ];

  const canContinue =
    step === 0 ? name.trim().length > 0 : step === 1 ? drafts.length > 0 : true;

  return (
    <div className="flex h-full flex-col bg-[var(--canvas)]">
      <div className="titlebar-drag h-11 shrink-0" />

      <div className="flex flex-1 items-center justify-center overflow-y-auto px-8 pb-8">
        <div key={step} className="animate-in w-full max-w-[560px]">
          {steps[step]}

          {error && (
            <div className="mt-5 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] px-4 py-3 text-[13px] text-[var(--danger)]">
              {error}
            </div>
          )}

          <div className="mt-9 flex items-center justify-between">
            <div className="flex gap-1.5">
              {steps.map((_, i) => (
                <span
                  key={i}
                  className={cx(
                    "h-1.5 rounded-full transition-all duration-300",
                    i === step ? "w-5 bg-[var(--ink-dim)]" : "w-1.5 bg-[var(--line)]",
                  )}
                />
              ))}
            </div>

            <div className="flex items-center gap-2">
              {step > 0 && (
                <Button variant="ghost" onClick={() => setStep(step - 1)}>
                  <ChevronLeft size={15} />
                  Back
                </Button>
              )}
              {step < steps.length - 1 ? (
                <Button variant="primary" disabled={!canContinue} onClick={() => setStep(step + 1)}>
                  Continue
                </Button>
              ) : (
                <Button variant="primary" disabled={saving} onClick={finish}>
                  {saving ? "Setting up…" : "Start using Retain"}
                </Button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function Heading({ title, body }: { title: string; body: string }) {
  return (
    <div className="mb-7">
      <h1 className="text-[26px] font-semibold tracking-[-0.02em] text-[var(--ink)]">{title}</h1>
      <p className="mt-2 text-[14px] leading-relaxed text-[var(--ink-dim)]">{body}</p>
    </div>
  );
}

function NameStep({ name, setName }: { name: string; setName: (v: string) => void }) {
  return (
    <>
      <Heading
        title="What should I call you?"
        body="Used for greetings and the odd bit of copy. It stays on this Mac."
      />
      <TextField
        autoFocus
        value={name}
        placeholder="Your name"
        onChange={(e) => setName(e.target.value)}
      />
    </>
  );
}

// ---------------------------------------------------------------------------

function SubjectsStep({
  drafts,
  setDrafts,
}: {
  drafts: Draft[];
  setDrafts: (d: Draft[]) => void;
}) {
  const [custom, setCustom] = useState("");
  const taken = drafts.map((d) => d.colour);
  const chosen = new Set(drafts.map((d) => d.name.toLowerCase()));

  const add = (subjectName: string, type?: SubjectType) => {
    const clean = subjectName.trim();
    if (!clean || drafts.length >= MAX || chosen.has(clean.toLowerCase())) return;
    setDrafts([
      ...drafts,
      {
        name: clean,
        colour: nextColour(taken),
        unitLevel: "1_2",
        subjectType: type ?? inferType(clean),
        typeLocked: false,
      },
    ]);
    setCustom("");
  };

  const patch = (index: number, changes: Partial<Draft>) =>
    setDrafts(drafts.map((d, i) => (i === index ? { ...d, ...changes } : d)));

  return (
    <>
      <Heading
        title={`Add your subjects${drafts.length ? ` (${drafts.length}/${MAX})` : ` (up to ${MAX})`}`}
        body="Units 3 & 4 subjects get exam countdowns, the topic tree and revision scheduling. Units 1 & 2 get timers and notes. All of this is editable later."
      />

      {drafts.length < MAX && (
        <div className="mb-5 flex flex-wrap gap-1.5">
          {VCE_SUBJECTS.filter((s) => !chosen.has(s.name.toLowerCase())).map((s) => (
            <button
              key={s.name}
              onClick={() => add(s.name, s.type)}
              className="rounded-full border border-[var(--line)] bg-[var(--surface)] px-3 py-1.5 text-[12.5px] text-[var(--ink-dim)] transition-all duration-[120ms] hover:border-[var(--ink-faint)] hover:text-[var(--ink)] active:scale-[0.97]"
            >
              {s.name}
            </button>
          ))}
        </div>
      )}

      {drafts.length < MAX && (
        <div className="mb-6 flex gap-2">
          <input
            value={custom}
            placeholder="Other subject…"
            onChange={(e) => setCustom(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add(custom)}
            className="h-9 flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] transition-colors focus:border-[var(--accent)]"
          />
          <Button onClick={() => add(custom)} disabled={!custom.trim()}>
            <Plus size={15} />
            Add
          </Button>
        </div>
      )}

      <div className="space-y-2.5">
        {drafts.map((d, i) => (
          <Card key={`${d.name}-${i}`} className="animate-in p-4">
            <div className="flex items-center gap-3">
              <ColourDot colour={d.colour} size={11} />
              <span className="flex-1 truncate text-[14px] font-medium text-[var(--ink)]">
                {d.name}
              </span>
              <button
                aria-label={`Remove ${d.name}`}
                onClick={() => setDrafts(drafts.filter((_, x) => x !== i))}
                className="text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
              >
                <X size={15} />
              </button>
            </div>

            <div className="mt-3.5 flex flex-wrap items-center gap-2">
              <Segmented
                size="sm"
                value={d.unitLevel}
                onChange={(v) => patch(i, { unitLevel: v })}
                options={[
                  { value: "1_2", label: UNIT_LEVEL_LABELS["1_2"] },
                  { value: "3_4", label: UNIT_LEVEL_LABELS["3_4"] },
                ]}
              />
              <Segmented
                size="sm"
                value={d.subjectType}
                onChange={(v) => patch(i, { subjectType: v, typeLocked: true })}
                options={(Object.keys(SUBJECT_TYPE_LABELS) as SubjectType[]).map((t) => ({
                  value: t,
                  label: SUBJECT_TYPE_LABELS[t],
                }))}
              />
            </div>

            <div className="mt-3 flex items-center gap-1.5">
              {PALETTE.map((c) => (
                <button
                  key={c}
                  aria-label={`Colour ${c}`}
                  onClick={() => patch(i, { colour: c })}
                  className={cx(
                    "h-5 w-5 rounded-full transition-all duration-[120ms]",
                    d.colour === c
                      ? "ring-2 ring-[var(--ink-dim)] ring-offset-2 ring-offset-[var(--surface)]"
                      : "opacity-55 hover:opacity-100",
                  )}
                  style={{ background: c }}
                />
              ))}
            </div>

            <p className="mt-3 text-[12px] leading-relaxed text-[var(--ink-faint)]">
              {SUBJECT_TYPE_BLURB[d.subjectType]}
            </p>
          </Card>
        ))}
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------

function NotificationsStep() {
  const [granted, setGranted] = useState<boolean | null>(null);
  const [asking, setAsking] = useState(false);

  const ask = async () => {
    setAsking(true);
    try {
      const already = await isPermissionGranted();
      setGranted(already ? true : (await requestPermission()) === "granted");
    } catch {
      setGranted(false);
    } finally {
      setAsking(false);
    }
  };

  return (
    <>
      <Heading
        title="Notifications"
        body="Retain only notifies you when something has actually changed — never on a schedule, never to tell you it's time to study."
      />

      <Card className="p-5">
        <ul className="space-y-3 text-[13.5px] leading-relaxed text-[var(--ink-dim)]">
          {[
            "Reviews are due — with the subject and topic, so you know what you're walking into.",
            "A topic hasn't been touched in a while and is worth a look.",
            "An assessment is approaching. Weekly at first, daily in the last fortnight.",
            "A Pomodoro block has finished.",
          ].map((line) => (
            <li key={line} className="flex gap-2.5">
              <Check size={15} className="mt-0.5 shrink-0 text-[var(--color-positive)]" />
              <span>{line}</span>
            </li>
          ))}
        </ul>

        <p className="mt-4 border-t border-[var(--line-soft)] pt-4 text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
          Capped at 3 a day by default, with quiet hours and per-category switches in Settings. You
          can skip this and turn it on whenever — everything works without it.
        </p>
      </Card>

      <div className="mt-4">
        {granted === true ? (
          <div className="flex items-center gap-2 text-[13px] text-[var(--color-positive)]">
            <Check size={15} />
            Notifications are on.
          </div>
        ) : granted === false ? (
          <div className="text-[13px] text-[var(--ink-faint)]">
            Not enabled. You can turn them on later in Settings, or in System Settings →
            Notifications.
          </div>
        ) : (
          <Button onClick={ask} disabled={asking}>
            {asking ? "Asking macOS…" : "Enable notifications"}
          </Button>
        )}
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------

function KeyStep() {
  const [provider, setProvider] = useState<Provider>("anthropic");
  const [present, setPresent] = useState(false);

  const label =
    PROVIDER_LABELS[provider];

  return (
    <>
      <Heading
        title="AI features, if you want them"
        body="Bring your own API key. It's used for a handful of narrow things — turning a messy note into a task, drafting cards from notes, a weekly review of where your time actually went."
      />

      <Card>
        <div className="px-5 pt-5">
          <Segmented
            size="sm"
            value={provider}
            onChange={async (v) => {
              setProvider(v);
              setPresent(await api.secretHas(v));
            }}
            options={(Object.keys(PROVIDER_LABELS) as Provider[]).map((p) => ({
              value: p,
              label: PROVIDER_LABELS[p],
            }))}
          />
        </div>

        <ApiKeyField
          provider={provider}
          label={label}
          present={present}
          onChange={async () => setPresent(await api.secretHas(provider))}
        />
      </Card>

      <p className="mt-4 text-[13px] leading-relaxed text-[var(--ink-faint)]">
        Retain checks the key with {label} before saving it, so a bad paste is caught now rather
        than the first time you use a feature. Every AI feature is optional and every other feature
        works without a key — skip this and the button below finishes setup either way.
      </p>
    </>
  );
}

const PROVIDER_LABELS: Record<Provider, string> = {
  anthropic: "Anthropic",
  open_ai: "OpenAI",
  gemini: "Gemini",
  open_router: "OpenRouter",
};

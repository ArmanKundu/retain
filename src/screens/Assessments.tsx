import { useCallback, useEffect, useState } from "react";
import { CalendarDays, Plus, Trash2 } from "lucide-react";

import { AiAction, AiGate, useAi } from "../components/Ai";
import {
  Button,
  Card,
  ColourDot,
  Empty,
  SectionTitle,
  cx,
} from "../components/ui";
import { api } from "../lib/api";
import { prettyDate } from "../lib/format";
import type {
  Assessment,
  AssessmentKind,
  GroundedText,
  TopicRow,
  TopicStatus,
} from "../lib/types";
import { useApp } from "../store";

/**
 * Assessments, countdowns, and retrospective revision.
 *
 * The bottom half is the important part and the easiest to get wrong. It is a
 * *ranking recomputed on every load*, not a plan: nothing here assigns a topic
 * to a future date, so there is nothing to fall behind on. The review points on
 * each assessment say *when* to revise; what to revise comes from the ranking on
 * the day.
 */

const KINDS: { value: AssessmentKind; label: string }[] = [
  { value: "sac", label: "SAC" },
  { value: "exam", label: "Exam" },
  { value: "other", label: "Other" },
];

function countdown(days: number): { text: string; urgent: boolean } {
  if (days < 0) return { text: `${Math.abs(days)}d ago`, urgent: false };
  if (days === 0) return { text: "Today", urgent: true };
  if (days === 1) return { text: "Tomorrow", urgent: true };
  return { text: `${days} days`, urgent: days <= 14 };
}

export function Assessments() {
  const subjects = useApp((s) => s.subjects);
  const setRoute = useApp((s) => s.setRoute);
  const [items, setItems] = useState<Assessment[]>([]);
  const [topics, setTopics] = useState<TopicStatus[]>([]);
  const [allTopics, setAllTopics] = useState<TopicRow[]>([]);
  const [adding, setAdding] = useState(false);
  const [subjectFilter, setSubjectFilter] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [list, ranked, all] = await Promise.all([
        api.listAssessments(false),
        api.surfaceTopics(subjectFilter, 12),
        api.listTopics(null),
      ]);
      setItems(list);
      setTopics(ranked);
      setAllTopics(all);
    } catch (e) {
      setError(String(e));
    }
  }, [subjectFilter]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 sm:px-9 pb-14">
      {/* Content scrolls under the title bar. macOS separates the two with a
          hard edge rather than letting text vanish mid-letter. */}
      <div className="titlebar-drag scroll-edge h-11" />

      <header className="mb-6 flex items-center">
        <div>
          <h1 className="text-[24px] font-semibold tracking-[var(--track-display)]">
            Assessments
          </h1>
          <p className="mt-1 text-[13.5px] text-[var(--ink-dim)]">
            Countdowns, and what's worth revising right now.
          </p>
        </div>
        <Button
          variant="primary"
          className="ml-auto"
          onClick={() => setAdding(true)}
        >
          <Plus size={15} />
          Add
        </Button>
      </header>

      {error && (
        <div className="mb-4 rounded-[var(--r-md)] border border-[color-mix(in_srgb,var(--danger)_30%,transparent)] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] px-4 py-3 text-[13px] text-[var(--danger)]">
          {error}
        </div>
      )}

      {adding && (
        <AddForm
          subjects={subjects}
          allTopics={allTopics}
          onClose={() => setAdding(false)}
          onSaved={async () => {
            setAdding(false);
            await load();
          }}
        />
      )}

      {/* Countdowns */}
      <section className="mb-7">
        <SectionTitle>Coming up</SectionTitle>
        {items.length === 0 ? (
          <Card className="mt-2.5">
            <Empty
              title="Nothing scheduled"
              body="Add a SAC or exam and Retain will count down to it, and work backwards to tell you when to start revising."
            />
          </Card>
        ) : (
          <div className="mt-2.5 space-y-2.5">
            {items.map((a) => {
              const c = countdown(a.daysAway);
              return (
                <Card key={a.id} className="lift p-4">
                  <div className="flex items-center gap-3">
                    <ColourDot colour={a.colour} size={9} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[14px] text-[var(--ink)]">
                        {a.name}
                      </div>
                      <div className="text-[12px] text-[var(--ink-faint)]">
                        {a.subjectName} · {a.kind.toUpperCase()} ·{" "}
                        {prettyDate(a.dueOn)}
                        {a.source === "compass" && " · from Compass"}
                      </div>
                    </div>
                    <div className="text-right">
                      <div
                        className={cx(
                          "tabular text-[26px] font-medium leading-none tracking-[var(--track-display)]",
                          c.urgent ? "text-[var(--warn)]" : "text-[var(--ink)]",
                        )}
                        style={
                          c.urgent
                            ? {
                                // A soft halo rather than a red badge. Urgency
                                // should register peripherally, not shout.
                                textShadow:
                                  "0 0 22px color-mix(in srgb, var(--warn) 40%, transparent)",
                              }
                            : undefined
                        }
                      >
                        {c.text}
                      </div>
                    </div>
                    <button
                      onClick={async () => {
                        await api.deleteAssessment(a.id);
                        await load();
                      }}
                      aria-label={`Delete ${a.name}`}
                      className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] hover:text-[var(--danger)]"
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>

                  {a.upcomingReviewPoints.length > 0 && (
                    <div className="mt-3 border-t border-[var(--line-soft)] pt-3">
                      <div className="flex items-center gap-1.5 text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">
                        <CalendarDays size={11} />
                        Revise on
                      </div>
                      <div className="mt-1.5 flex flex-wrap gap-1.5">
                        {a.upcomingReviewPoints.map((p) => (
                          <span
                            key={p}
                            className="rounded-full border border-[var(--line)] px-2 py-0.5 text-[11.5px] text-[var(--ink-dim)]"
                          >
                            {prettyDate(p).replace(/,.*$/, "")}
                          </span>
                        ))}
                      </div>
                      <p className="mt-2 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
                        These are when, not what. What to revise comes from the
                        ranking below on the day — nothing is pre-assigned, so
                        missing one costs nothing.
                      </p>
                    </div>
                  )}
                </Card>
              );
            })}
          </div>
        )}
      </section>

      {/* Retrospective revision */}
      <section>
        <div className="flex flex-wrap items-center gap-2">
          <SectionTitle>Worth revising now</SectionTitle>
          <div className="ml-auto flex gap-1.5">
            <button
              onClick={() => setSubjectFilter(null)}
              className={cx(
                "rounded-full border px-2.5 py-0.5 text-[11.5px] transition-colors",
                subjectFilter === null
                  ? "border-[var(--ink-faint)] text-[var(--ink)]"
                  : "border-[var(--line)] text-[var(--ink-faint)]",
              )}
            >
              All
            </button>
            {subjects.map((s) => (
              <button
                key={s.id}
                onClick={() => setSubjectFilter(s.id)}
                className={cx(
                  "rounded-full border px-2.5 py-0.5 text-[11.5px] transition-colors",
                  subjectFilter === s.id
                    ? "border-[var(--ink-faint)] text-[var(--ink)]"
                    : "border-[var(--line)] text-[var(--ink-faint)]",
                )}
              >
                {s.name}
              </button>
            ))}
          </div>
        </div>

        <Card className="mt-2.5">
          {topics.length === 0 ? (
            <Empty
              title="No topics yet"
              body="Add topics to a subject below, then rate your confidence each time you test yourself. Retain surfaces whatever you've left longest and felt shakiest about."
            />
          ) : (
            <div className="divide-y divide-[var(--line-soft)]">
              {topics.map((t) => (
                <TopicRowView key={t.topicId} topic={t} onLogged={load} />
              ))}
            </div>
          )}
        </Card>

        <TopicManager subjects={subjects} topics={allTopics} onChange={load} />
      </section>

      <PracticeQuestion
        onOpenSettings={() => setRoute("settings")}
        subjectId={subjectFilter}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------

/**
 * A VCAA-style question for a key knowledge dot point, with a model answer and
 * a mark-by-mark rubric.
 *
 * Both are hidden until you ask for them. Writing an answer and *then* marking
 * it is the entire exercise — a model answer visible while you write turns this
 * into copying, which is the same failure the blind re-attempt in the error log
 * exists to prevent.
 *
 * The generated question is a study aid, not a VCAA paper. It has not been
 * checked against the study design and shouldn't be treated as authoritative.
 */
function PracticeQuestion({
  onOpenSettings,
  subjectId,
}: {
  onOpenSettings: () => void;
  /** Scopes retrieval of your own material to one subject. */
  subjectId: number | null;
}) {
  const { status } = useAi();
  const [dotPoint, setDotPoint] = useState("");
  const [marks, setMarks] = useState(4);
  const [output, setOutput] = useState<GroundedText | null>(null);
  const [revealed, setRevealed] = useState(false);

  // The model is told to emit QUESTION / MODEL ANSWER / RUBRIC sections. Split
  // on those so the answer can stay hidden; if the shape isn't there, show the
  // whole thing rather than hiding something the user paid for.
  const split = output?.body.split(/^MODEL ANSWER\s*$/m);
  const question = split?.[0]?.replace(/^QUESTION\s*$/m, "").trim();
  const rest =
    split && split.length > 1
      ? split.slice(1).join("\nMODEL ANSWER\n").trim()
      : null;

  return (
    <section className="animate-in mt-7">
      <SectionTitle>Practice question</SectionTitle>
      <Card className="mt-2.5 p-5">
        <AiGate
          status={status}
          what="write a VCAA-style question on a dot point, with a model answer and a mark-by-mark rubric"
          onOpenSettings={onOpenSettings}
        >
          <textarea
            value={dotPoint}
            onChange={(e) => {
              setDotPoint(e.target.value);
              setOutput(null);
            }}
            placeholder="Paste a key knowledge dot point — e.g. “the role of enzymes in the synthesis of biomacromolecules”"
            className="selectable h-20 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 text-[12.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)]"
          />

          <div className="mt-3 flex items-center gap-3">
            <label className="text-[12.5px] text-[var(--ink-dim)]">Worth</label>
            <input
              type="number"
              min={1}
              max={10}
              value={marks}
              onChange={(e) => setMarks(Number(e.target.value))}
              className="tabular h-8 w-14 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-center text-[13px] text-[var(--ink)] focus:border-[var(--accent)]"
            />
            <span className="text-[12.5px] text-[var(--ink-dim)]">marks</span>

            <AiAction
              className="ml-auto"
              label={output ? "Another one" : "Write a question"}
              disabled={dotPoint.trim().length < 12}
              run={() => api.aiPracticeQuestion(dotPoint, marks, subjectId)}
              onDone={(result) => {
                setOutput(result);
                setRevealed(false);
              }}
            />
          </div>

          {output && (
            <div className="mt-4 border-t border-[var(--line-soft)] pt-4">
              <pre className="selectable whitespace-pre-wrap font-sans text-[13px] leading-[1.65] text-[var(--ink)]">
                {question || output.body}
              </pre>

              {rest &&
                (revealed ? (
                  <pre className="selectable mt-4 whitespace-pre-wrap border-t border-[var(--line-soft)] pt-4 font-sans text-[12.5px] leading-[1.65] text-[var(--ink-dim)]">
                    {rest}
                  </pre>
                ) : (
                  <Button
                    size="sm"
                    className="mt-4"
                    onClick={() => setRevealed(true)}
                  >
                    Show the answer and rubric
                  </Button>
                ))}

              {output.sources.length > 0 && (
                <div className="mt-4 border-t border-[var(--line-soft)] pt-3">
                  <div className="text-[11px] text-[var(--ink-faint)]">
                    Written using your own material:
                  </div>
                  <div className="mt-1.5 flex flex-wrap gap-1.5">
                    {output.sources.map((src, i) => (
                      <span
                        key={`${src.resourceId}-${src.ordinal}-${i}`}
                        title={src.content.slice(0, 400)}
                        className="rounded-full border border-[var(--line)] px-2 py-0.5 text-[11px] text-[var(--ink-dim)]"
                      >
                        {src.resourceTitle}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              <p className="mt-4 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
                Generated practice, not a VCAA question. It hasn't been checked
                against the study design — treat the rubric as a guide, not a
                mark scheme.
              </p>
            </div>
          )}
        </AiGate>
      </Card>
    </section>
  );
}

function TopicRowView({
  topic,
  onLogged,
}: {
  topic: TopicStatus;
  onLogged: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);

  const log = async (confidence: number) => {
    setBusy(true);
    try {
      await api.logTopicReview(topic.topicId, confidence);
      await onLogged();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-3 px-5 py-3">
      <ColourDot colour={topic.colour} size={8} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13.5px] text-[var(--ink)]">
          {topic.topicName}
        </div>
        <div className="text-[11.5px] text-[var(--ink-faint)]">
          {topic.subjectName}
          {topic.daysSince === null ? (
            <span className="ml-1.5 text-[var(--accent)]">never tested</span>
          ) : (
            <>
              {" · "}
              {topic.daysSince === 0 ? "today" : `${topic.daysSince}d ago`}
              {topic.lastConfidence !== null &&
                ` · felt ${topic.lastConfidence}/5`}
            </>
          )}
        </div>
      </div>

      {/* Confidence is logged after you test yourself, never predicted before. */}
      <div className="flex items-center gap-1">
        {[1, 2, 3, 4, 5].map((n) => (
          <button
            key={n}
            disabled={busy}
            onClick={() => void log(n)}
            title={`Tested it — felt ${n}/5`}
            className="h-7 w-7 rounded-[var(--r-sm)] border border-[var(--line)] text-[12px] text-[var(--ink-dim)] transition-all hover:border-[var(--accent)] hover:text-[var(--accent)] active:scale-[0.94] disabled:opacity-40"
          >
            {n}
          </button>
        ))}
      </div>
    </div>
  );
}

function TopicManager({
  subjects,
  topics,
  onChange,
}: {
  subjects: ReturnType<typeof useApp.getState>["subjects"];
  topics: TopicRow[];
  onChange: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [subjectId, setSubjectId] = useState(subjects[0]?.id ?? 0);
  const [name, setName] = useState("");

  const add = async () => {
    if (!name.trim()) return;
    await api.createTopic(subjectId, name);
    setName("");
    await onChange();
  };

  return (
    <div className="mt-3">
      <button
        onClick={() => setOpen(!open)}
        className="text-[12.5px] text-[var(--ink-faint)] hover:text-[var(--ink)]"
      >
        {open ? "Hide topics" : `Manage topics (${topics.length})`}
      </button>

      {open && (
        <Card className="animate-in mt-2.5 p-4">
          <div className="flex flex-wrap gap-2">
            <select
              value={subjectId}
              onChange={(e) => setSubjectId(Number(e.target.value))}
              className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
            >
              {subjects.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void add()}
              placeholder="Topic name — e.g. Immunity"
              className="h-8 flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13px] text-[var(--ink)]"
            />
            <Button size="sm" onClick={add} disabled={!name.trim()}>
              Add topic
            </Button>
          </div>

          <p className="mt-2 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
            Topics you add by hand. The VCAA Biology tree will populate these
            automatically once it's built.
          </p>

          {topics.length > 0 && (
            <div className="mt-3 flex flex-wrap gap-1.5">
              {topics.map((t) => (
                <span
                  key={t.id}
                  className="flex items-center gap-1.5 rounded-full border border-[var(--line)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)]"
                >
                  {t.name}
                  <button
                    onClick={async () => {
                      await api.deleteTopic(t.id);
                      await onChange();
                    }}
                    className="text-[var(--ink-faint)] hover:text-[var(--danger)]"
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}

function AddForm({
  subjects,
  allTopics,
  onClose,
  onSaved,
}: {
  subjects: ReturnType<typeof useApp.getState>["subjects"];
  allTopics: TopicRow[];
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const [subjectId, setSubjectId] = useState(subjects[0]?.id ?? 0);
  const [name, setName] = useState("");
  const [kind, setKind] = useState<AssessmentKind>("sac");
  const [dueOn, setDueOn] = useState("");
  const [topicIds, setTopicIds] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);

  const subjectTopics = allTopics.filter((t) => t.subjectId === subjectId);

  const save = async () => {
    setError(null);
    try {
      await api.createAssessment({
        subjectId,
        name,
        kind,
        dueOn,
        topicIds: topicIds.length ? topicIds : null,
      });
      await onSaved();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Card className="animate-in mb-5 p-5">
      <div className="flex flex-wrap gap-2">
        <select
          value={subjectId}
          onChange={(e) => {
            setSubjectId(Number(e.target.value));
            setTopicIds([]);
          }}
          className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
        >
          {subjects.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>

        <select
          value={kind}
          onChange={(e) => setKind(e.target.value as AssessmentKind)}
          className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
        >
          {KINDS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>

        <input
          type="date"
          value={dueOn}
          onChange={(e) => setDueOn(e.target.value)}
          className="h-8 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)]"
        />
      </div>

      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Unit 3 AOS1 SAC"
        className="mt-2.5 h-8 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[13px] text-[var(--ink)]"
      />

      {subjectTopics.length > 0 && (
        <div className="mt-3">
          <div className="text-[11px] uppercase tracking-[0.07em] text-[var(--ink-faint)]">
            Topics covered
          </div>
          <div className="mt-1.5 flex flex-wrap gap-1.5">
            {subjectTopics.map((t) => (
              <button
                key={t.id}
                onClick={() =>
                  setTopicIds((prev) =>
                    prev.includes(t.id)
                      ? prev.filter((x) => x !== t.id)
                      : [...prev, t.id],
                  )
                }
                className={cx(
                  "rounded-full border px-2.5 py-1 text-[12px] transition-colors",
                  topicIds.includes(t.id)
                    ? "border-[var(--accent)] text-[var(--accent)]"
                    : "border-[var(--line)] text-[var(--ink-faint)]",
                )}
              >
                {t.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {error && (
        <div className="mt-3 text-[12.5px] text-[var(--danger)]">{error}</div>
      )}

      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="primary"
          disabled={!name.trim() || !dueOn}
          onClick={save}
        >
          Add assessment
        </Button>
      </div>
    </Card>
  );
}

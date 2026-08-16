// The library: your material, and everything Retain has written from it.
//
// Two halves that belong together. **Materials** is what you supply — the study
// design, past papers, your own notes. **Saved** is what the AI produced, kept
// automatically rather than behind a save button you'd have to remember.
//
// The connection between them is the point: a note generated while your study
// design is loaded is grounded in the real document, and the excerpts it used
// are shown so you can check them.

import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ChevronRight,
  Download,
  FileText,
  FolderOpen,
  FolderSync,
  Pin,
  Printer,
  Search,
  Trash2,
  Upload,
} from "lucide-react";

import { AiAction, AiGate, useAi } from "../components/Ai";
import { Markdown } from "../components/Markdown";
import { Chip, SectionHeader, SubjectPill } from "../components/primitives";
import { Button, Card, Empty, cx } from "../components/ui";
import { api } from "../lib/api";
import type {
  Excerpt,
  GroundedText,
  ImportedFile,
  LibraryItem,
  LibraryKind,
  Resource,
  ResourceKind,
  SubjectFolder,
} from "../lib/types";
import { useApp } from "../store";

const KIND_LABEL: Record<LibraryKind, string> = {
  notes: "Notes",
  practice_question: "Practice",
  weekly_review: "Weekly review",
  answer: "Answer",
  cards: "Cards",
};

/**
 * The filing system, in the order Retain trusts them.
 *
 * Order is not cosmetic: an answer reads the study design before your own
 * notes, because the first says what's examinable and the second records what
 * you understood at the time — which is the thing you're trying to correct.
 */
/**
 * Ordered by authority, which is the thing the taxonomy is actually for: the
 * study design says what is examinable, a report says what earned marks, a
 * trial exam is a school's prediction, your notes record what you understood.
 * The assistant weights them in this order.
 */
const RESOURCE_KINDS: { value: ResourceKind; label: string; hint: string }[] = [
  {
    value: "study_design",
    label: "Study design",
    hint: "What VCAA says is examinable",
  },
  { value: "past_paper", label: "Past papers", hint: "VCAA exams" },
  {
    value: "exam_solution",
    label: "Solutions",
    hint: "Marking schemes, examiner's reports",
  },
  {
    value: "trial_test",
    label: "Trial tests",
    hint: "Your school's practice exams — not VCAA",
  },
  { value: "textbook", label: "Textbook", hint: "Chapters and extracts" },
  { value: "school_notes", label: "School notes", hint: "From your teacher" },
  { value: "personal_notes", label: "My notes", hint: "Your own" },
  { value: "other", label: "Other", hint: "Anything else" },
];

/**
 * Which kinds are filed per unit.
 *
 * Mirrors `ResourceKind::per_unit` in Rust. A study design covers the whole
 * sequence and a VCAA exam examines both units in one paper, so asking which
 * unit they belong to has no answer — those stay unit-less, and that is a real
 * answer rather than missing data.
 */
const PER_UNIT: ResourceKind[] = [
  "school_notes",
  "personal_notes",
  "trial_test",
];

export function Library() {
  const [tab, setTab] = useState<"saved" | "materials">("saved");

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 pb-16 sm:px-9">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-6">
        <h1 className="text-[28px] font-semibold tracking-[-0.028em]">
          Library
        </h1>
        <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
          Everything Retain has written for you, and the material it writes
          from.
        </p>
      </header>

      <div className="animate-rise mb-6 flex gap-1.5">
        <Chip active={tab === "saved"} onClick={() => setTab("saved")}>
          Saved
        </Chip>
        <Chip active={tab === "materials"} onClick={() => setTab("materials")}>
          Materials
        </Chip>
      </div>

      {tab === "saved" ? <Saved /> : <Materials />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Saved AI output
// ---------------------------------------------------------------------------

function Saved() {
  const subjects = useApp((s) => s.subjects);
  const { status } = useAi();
  const setRoute = useApp((s) => s.setRoute);

  const [items, setItems] = useState<LibraryItem[]>([]);
  const [search, setSearch] = useState("");
  const [kind, setKind] = useState<string | null>(null);
  const [open, setOpen] = useState<LibraryItem | null>(null);
  const [topic, setTopic] = useState("");
  const [subjectId, setSubjectId] = useState<number | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setItems(await api.listLibrary({ search: search || null, kind }));
    } catch {
      setItems([]);
    }
  }, [search, kind]);

  useEffect(() => {
    const t = setTimeout(() => void load(), 140);
    return () => clearTimeout(t);
  }, [load]);

  const kinds = useMemo(
    () => Array.from(new Set(items.map((i) => i.kind))).sort(),
    [items],
  );

  return (
    <>
      {/* Ask for notes. The one place in the app that generates a document
          rather than a card or a suggestion. */}
      <section className="animate-rise mb-8">
        <SectionHeader
          title="Write me notes"
          hint="grounded in your own material"
        />
        <Card className="p-5">
          <AiGate
            status={status}
            what="write structured study notes on a topic, using the material you've uploaded"
            onOpenSettings={() => setRoute("settings")}
          >
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={topic}
                onChange={(e) => setTopic(e.target.value)}
                placeholder="e.g. the role of enzymes in DNA replication"
                className="h-9 min-w-[240px] flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
              />
              <select
                value={subjectId ?? ""}
                onChange={(e) =>
                  setSubjectId(e.target.value ? Number(e.target.value) : null)
                }
                className="h-9 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[12.5px] text-[var(--ink)]"
              >
                <option value="">Any subject</option>
                {subjects.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>

              <AiAction<GroundedText>
                label="Write notes"
                disabled={topic.trim().length < 4}
                run={() => api.aiNotes(topic, subjectId)}
                onDone={async (r) => {
                  setTopic("");
                  await load();
                  setNote(
                    r.sources.length > 0
                      ? `Saved, using ${r.sources.length} passage${r.sources.length === 1 ? "" : "s"} of your material.`
                      : "Saved. No uploaded material matched, so this is from the model's own knowledge.",
                  );
                }}
              />
            </div>

            {note && (
              <p className="mt-3 text-[12.5px] text-[var(--ink-dim)]">{note}</p>
            )}
          </AiGate>
        </Card>
      </section>

      <section className="animate-rise">
        <SectionHeader title="Saved" hint={`${items.length} items`} />

        <div className="mb-3 flex flex-wrap items-center gap-2">
          <div className="relative min-w-[200px] flex-1">
            <Search
              size={13}
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--ink-faint)]"
            />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search titles and content…"
              className="h-8 w-full rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] pl-8 pr-3 text-[12.5px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
            />
          </div>
          <Chip active={kind === null} onClick={() => setKind(null)}>
            All
          </Chip>
          {kinds.map((k) => (
            <Chip key={k} active={kind === k} onClick={() => setKind(k)}>
              {KIND_LABEL[k]}
            </Chip>
          ))}
        </div>

        {items.length === 0 ? (
          <Card>
            <Empty
              title={search || kind ? "Nothing matches" : "Nothing saved yet"}
              body={
                search || kind
                  ? "Try a different search, or clear the filter."
                  : "Ask for notes above, generate a practice question, or run a weekly review — everything the AI writes is kept here automatically."
              }
            />
          </Card>
        ) : (
          <div className="-mx-3">
            {items.map((item) => (
              <button
                key={item.id}
                onClick={() => setOpen(item)}
                className="group flex w-full items-center gap-3 rounded-[var(--r-md)] px-3 py-3 text-left transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
              >
                {item.pinned && (
                  <Pin size={12} className="shrink-0 text-[var(--warn)]" />
                )}

                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13.5px] text-[var(--ink)]">
                    {item.title}
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 text-[11.5px] text-[var(--ink-faint)]">
                    <span>{KIND_LABEL[item.kind]}</span>
                    <span>·</span>
                    <span>{item.createdAt.slice(0, 10)}</span>
                    {item.model && (
                      <>
                        <span>·</span>
                        <span className="truncate font-mono">{item.model}</span>
                      </>
                    )}
                  </div>
                </div>

                {item.subjectName && item.colour && (
                  <SubjectPill
                    name={item.subjectName}
                    colour={item.colour}
                    size="sm"
                  />
                )}
              </button>
            ))}
          </div>
        )}
      </section>

      {open && (
        <ItemViewer
          item={open}
          onClose={() => setOpen(null)}
          onChanged={async () => {
            await load();
            setOpen(null);
          }}
        />
      )}
    </>
  );
}

/**
 * One saved item, full size.
 *
 * Print uses the browser's own dialog against a print stylesheet — a real PDF
 * writer would be a dependency and a worse result than the one macOS already
 * has in its print sheet.
 */
function ItemViewer({
  item,
  onClose,
  onChanged,
}: {
  item: LibraryItem;
  onClose: () => void;
  onChanged: () => Promise<void>;
}) {
  const [saved, setSaved] = useState<string | null>(null);

  return (
    <div
      className="scrim fixed inset-0 z-50 flex items-center justify-center px-8"
      onClick={onClose}
    >
      <div
        className="sheet animate-pop flex max-h-[86vh] w-full max-w-[820px] flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-3 px-9 pb-5 pt-8">
          <div className="min-w-0 flex-1">
            <h2 className="text-[27px] font-semibold leading-tight tracking-[-0.025em]">
              {item.title}
            </h2>
            {/* Metadata as quiet pills rather than a run-on line of separators. */}
            <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
              <Meta>{KIND_LABEL[item.kind]}</Meta>
              {item.subjectName && <Meta>{item.subjectName}</Meta>}
              <Meta>{item.createdAt.slice(0, 10)}</Meta>
              {item.model && <Meta mono>{item.model}</Meta>}
            </div>
          </div>

          <button
            onClick={async () => {
              await api.setLibraryPinned(item.id, !item.pinned);
              await onChanged();
            }}
            aria-label={item.pinned ? "Unpin" : "Pin to the top"}
            title={item.pinned ? "Unpin" : "Pin to the top"}
            className={cx(
              "pressable rounded-[var(--r-sm)] p-1.5",
              item.pinned
                ? "text-[var(--warn)]"
                : "text-[var(--ink-faint)] hover:text-[var(--ink)]",
            )}
          >
            <Pin size={14} />
          </button>
        </div>

        {/* `print-target` is what the print stylesheet keeps; everything else on
            the page is hidden when printing. */}
        <div className="print-target selectable min-h-0 flex-1 overflow-y-auto px-9 pb-8">
          {/* Paper only. A printed page with no title is one you can't file,
              and a stack of untitled notes is why people stop printing them. */}
          <header className="print-header">
            <div className="print-title">{item.title}</div>
            <div className="print-meta">
              {[item.subjectName, new Date(item.createdAt).toLocaleDateString()]
                .filter(Boolean)
                .join(" · ")}
            </div>
          </header>

          {item.prompt && (
            <p className="print-hide mb-6 border-l-2 border-[var(--line)] pl-3.5 text-[13px] italic leading-relaxed text-[var(--ink-faint)]">
              {item.prompt}
            </p>
          )}
          {/* A measure of roughly 70 characters. Wider than this and the eye
              loses the line it was on between one row and the next. */}
          <Markdown source={item.body} className="max-w-[680px]" />
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-[var(--line-soft)] px-9 py-4">
          <Button
            size="sm"
            onClick={async () => {
              try {
                setSaved(`Saved to ${await api.exportLibraryItem(item.id)}`);
              } catch (e) {
                setSaved(String(e));
              }
            }}
          >
            <Download size={13} />
            Save as Markdown
          </Button>

          {/* macOS's print dialog has "Save as PDF" in its corner, so this is
              also the PDF export — and it goes through the same page stylesheet
              the printer would use, rather than a second renderer that drifts. */}
          <Button size="sm" onClick={() => window.print()}>
            <Printer size={13} />
            Print or save as PDF
          </Button>

          <Button
            size="sm"
            variant="danger"
            className="ml-auto"
            onClick={async () => {
              await api.deleteLibraryItem(item.id);
              await onChanged();
            }}
          >
            <Trash2 size={13} />
            Delete
          </Button>

          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </div>

        {saved && (
          <p className="px-4 pb-4 text-[12px] text-[var(--ink-dim)]">{saved}</p>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Materials
// ---------------------------------------------------------------------------

function Materials() {
  const subjects = useApp((s) => s.subjects);
  const [list, setList] = useState<Resource[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [probe, setProbe] = useState("");
  // The excerpts themselves, not a count. "6 passages match" tells you nothing
  // you can act on — which six documents, and what they actually say, is the
  // answer to "have I got anything on this".
  const [hits, setHits] = useState<Excerpt[] | null>(null);
  // Subjects start collapsed once there are enough of them to scroll past.
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const load = useCallback(async () => {
    try {
      setList(await api.listResources(null));
    } catch {
      setList([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const totalWords = list.reduce((n, r) => n + r.wordCount, 0);

  return (
    <>
      <section className="animate-rise mb-8">
        <SectionHeader
          title="Your material"
          hint={
            list.length > 0
              ? `${totalWords.toLocaleString()} words indexed`
              : undefined
          }
        />
        <Uploader subjects={subjects} onAdded={load} onError={setError} />
        {error && (
          <p className="mt-2 text-[12.5px] text-[var(--danger)]">{error}</p>
        )}
      </section>

      {list.length > 0 && (
        <section className="animate-rise mb-8">
          <SectionHeader
            title="Check coverage"
            hint="no model call, costs nothing"
          />
          <Card className="p-4">
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={probe}
                onChange={(e) => setProbe(e.target.value)}
                placeholder="Does my material cover… e.g. osmoregulation"
                className="h-8 min-w-[220px] flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[12.5px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
              />
              <Button
                size="sm"
                disabled={probe.trim().length < 3}
                onClick={async () =>
                  setHits(await api.searchResources(probe, null, 40))
                }
              >
                Check
              </Button>
            </div>
            {hits !== null && <Coverage hits={hits} />}
          </Card>
        </section>
      )}

      <section className="animate-rise">
        {list.length === 0 ? (
          <Card>
            <Empty
              title="No material yet"
              body="Add your study design, past papers and notes above. Once they're here, generated notes and practice questions are written from your actual documents instead of the model's memory of them."
            />
          </Card>
        ) : (
          <div className="-mx-3">
            {groupBySubject(list).map(([subject, items]) => (
              <div key={subject} className="mb-1">
                {/* One row per subject rather than 131 rows in one column. A
                    list you have to scroll past to reach anything isn't a
                    library, it's a directory listing. */}
                <button
                  onClick={() =>
                    setCollapsed((c) => {
                      const next = new Set(c);
                      if (next.has(subject)) next.delete(subject);
                      else next.add(subject);
                      return next;
                    })
                  }
                  className="flex w-full items-center gap-2 rounded-[var(--r-md)] px-3 py-2 text-left hover:bg-[var(--surface-hi)]/60"
                >
                  <ChevronRight
                    size={13}
                    className={cx(
                      "shrink-0 text-[var(--ink-faint)] transition-transform duration-[var(--t-fast)]",
                      !collapsed.has(subject) && "rotate-90",
                    )}
                  />
                  <span className="text-[13px] text-[var(--ink)]">
                    {subject}
                  </span>
                  <span className="text-[11.5px] text-[var(--ink-faint)]">
                    {items.length}{" "}
                    {items.length === 1 ? "document" : "documents"} ·{" "}
                    {items
                      .reduce((n, r) => n + r.wordCount, 0)
                      .toLocaleString()}{" "}
                    words
                  </span>
                </button>

                {!collapsed.has(subject) &&
                  items.map((r) => (
                    <div
                      key={r.id}
                      className="group flex items-center gap-3 rounded-[var(--r-md)] px-3 py-3 transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
                    >
                      <FileText
                        size={14}
                        className="shrink-0 text-[var(--ink-faint)]"
                      />

                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[13.5px] text-[var(--ink)]">
                          {r.title}
                        </div>
                        <div className="mt-0.5 flex items-center gap-2 text-[11.5px] text-[var(--ink-faint)]">
                          <span>
                            {RESOURCE_KINDS.find((k) => k.value === r.kind)
                              ?.label ?? r.kind}
                          </span>
                          <span>·</span>
                          <span>{r.wordCount.toLocaleString()} words</span>
                          <span>·</span>
                          <span>{r.chunkCount} passages</span>
                        </div>
                      </div>

                      {r.subjectName && (
                        <span className="shrink-0 text-[11.5px] text-[var(--ink-faint)]">
                          {r.subjectName}
                        </span>
                      )}

                      <button
                        onClick={async () => {
                          await api.deleteResource(r.id);
                          await load();
                        }}
                        aria-label={`Remove ${r.title}`}
                        className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] opacity-0 transition-all duration-[var(--t-fast)] hover:text-[var(--danger)] group-hover:opacity-100 focus-visible:opacity-100"
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  ))}
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}

/**
 * Documents by subject, ordered by authority within each.
 *
 * Authority order matters more than alphabetical: what you want to see first
 * when you open Chemistry is the study design, not "2022 neap units 3 4".
 */
function groupBySubject(list: Resource[]): [string, Resource[]][] {
  const order = RESOURCE_KINDS.map((k) => k.value);
  const groups = new Map<string, Resource[]>();

  for (const r of list) {
    const key = r.subjectName ?? "Unfiled";
    const bucket = groups.get(key);
    if (bucket) bucket.push(r);
    else groups.set(key, [r]);
  }

  for (const items of groups.values()) {
    items.sort(
      (a, b) =>
        order.indexOf(a.kind) - order.indexOf(b.kind) ||
        a.title.localeCompare(b.title),
    );
  }

  // Unfiled last — it's the bucket for things that haven't been sorted, not a
  // subject you study.
  return [...groups.entries()].sort(([a], [b]) =>
    a === "Unfiled" ? 1 : b === "Unfiled" ? -1 : a.localeCompare(b),
  );
}

/**
 * What a coverage check actually found.
 *
 * It used to report a number. "6 passages match" is not an answer to "have I
 * got anything on this" — six passages of the study design and six of a past
 * paper mean different things, and one strong hit in the right document beats
 * six weak ones scattered across your notes. So this names the documents,
 * counts the hits in each, and shows the best passage from the most
 * authoritative one so you can see whether it is actually relevant.
 */
function Coverage({ hits }: { hits: Excerpt[] }) {
  if (hits.length === 0) {
    return (
      <p className="mt-2.5 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
        Nothing in your material matches those words. A generated answer on this
        would come from the model's own knowledge rather than your documents.
      </p>
    );
  }

  const byDoc = new Map<
    number,
    { title: string; kind: ResourceKind; hits: Excerpt[] }
  >();
  for (const h of hits) {
    const found = byDoc.get(h.resourceId);
    if (found) found.hits.push(h);
    else
      byDoc.set(h.resourceId, {
        title: h.resourceTitle,
        kind: h.kind,
        hits: [h],
      });
  }

  // Already returned in authority order by the backend, so the first document
  // is the one an answer would lean on hardest.
  const docs = [...byDoc.values()];

  return (
    <div className="mt-3">
      <p className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
        {hits.length} {hits.length === 1 ? "passage" : "passages"} across{" "}
        {docs.length} {docs.length === 1 ? "document" : "documents"}. An answer
        would be grounded in these.
      </p>

      <ul className="mt-2 space-y-1">
        {docs.slice(0, 6).map((d, i) => (
          <li
            key={d.title}
            className="rounded-[var(--r-sm)] bg-[var(--surface-hi)]/60 px-3 py-2"
          >
            <div className="flex items-baseline gap-2">
              <span className="truncate text-[12.5px] text-[var(--ink)]">
                {d.title}
              </span>
              <span className="shrink-0 text-[11px] text-[var(--ink-faint)]">
                {RESOURCE_KINDS.find((k) => k.value === d.kind)?.label ??
                  d.kind}{" "}
                · {d.hits.length}
              </span>
            </div>
            {/* Only the strongest document gets a passage. Six snippets is the
                wall this replaced. */}
            {i === 0 && (
              <p className="mt-1 line-clamp-2 text-[11.5px] leading-relaxed text-[var(--ink-faint)]">
                {d.hits[0].content}
              </p>
            )}
          </li>
        ))}
        {docs.length > 6 && (
          <li className="px-3 text-[11.5px] text-[var(--ink-faint)]">
            and {docs.length - 6} more
          </li>
        )}
      </ul>
    </div>
  );
}

/**
 * Add material: subject folders, a folder anywhere, individual files, or paste.
 *
 * The folder route is the one that matters. Retain makes
 * `~/Documents/Retain/<Subject>/` for each of your subjects; drop a term's PDFs
 * into the right one and press Sync. Everything else is a fallback for material
 * that doesn't live in a folder.
 */
function Uploader({
  subjects,
  onAdded,
  onError,
}: {
  subjects: ReturnType<typeof useApp.getState>["subjects"];
  onAdded: () => Promise<void>;
  onError: (e: string | null) => void;
}) {
  const [title, setTitle] = useState("");
  const [kind, setKind] = useState<ResourceKind>("study_design");
  const [unit, setUnit] = useState<number | null>(null);
  const [subjectId, setSubjectId] = useState<number | null>(null);
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState(false);
  const [folders, setFolders] = useState<SubjectFolder[]>([]);
  const [report, setReport] = useState<ImportedFile[] | null>(null);
  const [progress, setProgress] = useState<string | null>(null);

  const loadFolders = useCallback(async () => {
    try {
      setFolders(await api.ensureSubjectFolders());
    } catch (e) {
      onError(String(e));
    }
  }, [onError]);

  useEffect(() => {
    void loadFolders();
  }, [loadFolders]);

  const runImport = async (
    path: string,
    forSubject: number | null,
    label: string,
  ) => {
    setBusy(true);
    setProgress(`Reading ${label}…`);
    onError(null);
    try {
      const result = await api.importFolder(path, forSubject);
      setReport(result);
      await onAdded();
      await loadFolders();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const pickFolder = async () => {
    const picked = await openDialog({
      directory: true,
      title: "Choose a folder of material",
    });
    if (typeof picked === "string") {
      await runImport(picked, subjectId, "that folder");
    }
  };

  const pickFiles = async () => {
    const picked = await openDialog({
      multiple: true,
      title: "Add files",
      filters: [
        {
          name: "Documents",
          extensions: [
            "pdf",
            "txt",
            "md",
            "markdown",
            "csv",
            "html",
            "rtf",
            "json",
            "tex",
          ],
        },
      ],
    });
    if (!picked) return;

    setBusy(true);
    onError(null);
    const results: ImportedFile[] = [];

    try {
      for (const path of Array.isArray(picked) ? picked : [picked]) {
        setProgress(`Reading ${path.split("/").pop()}…`);
        const outcome = await api.readFileText(path);

        if (outcome.status === "extracted") {
          const name = outcome.name.replace(/\.[^.]+$/, "");
          await api.addResource(
            subjectId,
            title.trim() || name,
            kind,
            unit,
            outcome.name,
            outcome.text,
          );
          results.push({
            name: outcome.name,
            outcome,
            resourceId: null,
            skippedDuplicate: false,
          });
        } else {
          results.push({
            name: outcome.name,
            outcome,
            resourceId: null,
            skippedDuplicate: false,
          });
        }
      }
      setReport(results);
      setTitle("");
      await onAdded();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const commitPaste = async () => {
    setBusy(true);
    onError(null);
    try {
      await api.addResource(
        subjectId,
        title.trim() || "Pasted material",
        kind,
        unit,
        "pasted",
        pasted,
      );
      setTitle("");
      setPasted("");
      await onAdded();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      {/* The subject folders. */}
      {folders.length > 0 && (
        <Card className="mb-3 p-5">
          <div className="flex items-baseline gap-2">
            <h3 className="text-[13.5px] font-medium">Your subject folders</h3>
            <span className="text-[11.5px] text-[var(--ink-faint)]">
              in Documents › Retain
            </span>
          </div>
          <p className="mt-1.5 text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
            Drop notes, past papers and the study design into the matching
            folder, then press Sync. Files already read are skipped, so syncing
            again is cheap.
          </p>

          <div className="mt-3.5 space-y-1">
            {folders.map((f) => {
              const pending = f.fileCount - f.importedCount;
              return (
                <div
                  key={f.subjectId}
                  className="group flex items-center gap-3 rounded-[var(--r-md)] px-2 py-2 transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
                >
                  <SubjectPill name={f.subjectName} colour={f.colour} dotOnly />

                  <button
                    onClick={() =>
                      void api.revealFolder(f.path).catch(() => {})
                    }
                    title="Show this folder in Finder"
                    className="pressable min-w-0 flex-1 text-left"
                  >
                    <span className="block truncate text-[13px] text-[var(--ink)]">
                      {f.subjectName}
                    </span>
                    <span className="block text-[11.5px] text-[var(--ink-faint)]">
                      {f.fileCount === 0
                        ? "empty — drop files in"
                        : pending > 0
                          ? `${pending} new of ${f.fileCount} file${f.fileCount === 1 ? "" : "s"}`
                          : `${f.fileCount} file${f.fileCount === 1 ? "" : "s"}, all read`}
                    </span>
                  </button>

                  <Button
                    size="sm"
                    variant={pending > 0 ? "primary" : "ghost"}
                    disabled={busy || f.fileCount === 0}
                    onClick={() =>
                      void runImport(f.path, f.subjectId, f.subjectName)
                    }
                  >
                    <FolderSync size={13} />
                    Sync
                  </Button>
                </div>
              );
            })}
          </div>
        </Card>
      )}

      <Card className="p-5">
        <div className="flex flex-wrap items-center gap-2">
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Title — optional; taken from the filename if blank"
            className="h-9 min-w-[240px] flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
          />
          <select
            value={subjectId ?? ""}
            onChange={(e) =>
              setSubjectId(e.target.value ? Number(e.target.value) : null)
            }
            className="h-9 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2.5 text-[12.5px] text-[var(--ink)]"
          >
            <option value="">Any subject</option>
            {subjects.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>

        <div className="mt-3 flex flex-wrap gap-1.5">
          {RESOURCE_KINDS.map((k) => (
            <Chip
              key={k.value}
              active={kind === k.value}
              onClick={() => {
                setKind(k.value);
                // A kind that spans the sequence can't carry a unit, so any
                // previous choice is cleared rather than silently kept.
                if (!PER_UNIT.includes(k.value)) setUnit(null);
              }}
              title={k.hint}
            >
              {k.label}
            </Chip>
          ))}
        </div>

        {/* Shown only for the kinds where the question has an answer. */}
        {PER_UNIT.includes(kind) && (
          <div className="animate-rise mt-2.5 flex flex-wrap items-center gap-1.5">
            <span className="mr-1 text-[12px] text-[var(--ink-faint)]">
              Unit
            </span>
            {[3, 4].map((u) => (
              <Chip
                key={u}
                active={unit === u}
                onClick={() => setUnit(unit === u ? null : u)}
              >
                {u}
              </Chip>
            ))}
            {[1, 2].map((u) => (
              <Chip
                key={u}
                active={unit === u}
                onClick={() => setUnit(unit === u ? null : u)}
              >
                {u}
              </Chip>
            ))}
            <span className="ml-1 text-[11.5px] text-[var(--ink-faint)]">
              leave blank if it covers both
            </span>
          </div>
        )}

        <div className="mt-3.5 flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant="primary"
            disabled={busy}
            onClick={() => void pickFolder()}
          >
            <FolderOpen size={13} />
            Add a folder
          </Button>
          <Button size="sm" disabled={busy} onClick={() => void pickFiles()}>
            <Upload size={13} />
            Add files
          </Button>
          <span className="text-[11.5px] text-[var(--ink-faint)]">
            PDF, text, Markdown, HTML, RTF, CSV
          </span>
        </div>

        {progress && (
          <p className="mt-2.5 text-[12.5px] text-[var(--ink-dim)]">
            {progress}
          </p>
        )}

        <details className="mt-4 border-t border-[var(--line-soft)] pt-3.5">
          <summary className="cursor-pointer text-[12.5px] text-[var(--ink-dim)]">
            Or paste text directly
          </summary>
          <textarea
            value={pasted}
            onChange={(e) => setPasted(e.target.value)}
            placeholder="Paste from anywhere — a website, a scanned page you've OCR'd, a message."
            className="selectable mt-2.5 h-28 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 text-[12.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
          />
          <Button
            size="sm"
            className="mt-2.5"
            disabled={busy || pasted.trim().length < 40}
            onClick={() => void commitPaste()}
          >
            Add pasted text
          </Button>
        </details>
      </Card>

      {report && (
        <ImportReport report={report} onClose={() => setReport(null)} />
      )}
    </>
  );
}

/**
 * What each file produced.
 *
 * Shown in full rather than as a count, because the interesting cases are the
 * failures: a scanned PDF looks identical to a successful import until you go
 * looking for its content and it isn't there.
 */
function ImportReport({
  report,
  onClose,
}: {
  report: ImportedFile[];
  onClose: () => void;
}) {
  const added = report.filter(
    (r) => r.outcome.status === "extracted" && !r.skippedDuplicate,
  );
  const skipped = report.filter((r) => r.skippedDuplicate);
  const problems = report.filter((r) => r.outcome.status !== "extracted");

  return (
    <Card className="animate-rise mt-3 p-5">
      <div className="flex items-baseline gap-3">
        <h3 className="text-[13.5px] font-medium">
          {added.length} added
          {skipped.length > 0 && `, ${skipped.length} already there`}
          {problems.length > 0 && `, ${problems.length} couldn't be read`}
        </h3>
        <button
          onClick={onClose}
          className="pressable ml-auto text-[12px] text-[var(--ink-faint)] hover:text-[var(--ink)]"
        >
          Dismiss
        </button>
      </div>

      {problems.length > 0 && (
        <div className="mt-3 space-y-2">
          {problems.map((p, i) => (
            <div key={`${p.name}-${i}`} className="flex items-start gap-2">
              <AlertTriangle
                size={13}
                className="mt-[2px] shrink-0 text-[var(--warn)]"
              />
              <div className="min-w-0 text-[12.5px] leading-relaxed">
                <span className="text-[var(--ink)]">{p.name}</span>
                <span className="text-[var(--ink-dim)]">
                  {" — "}
                  {p.outcome.status === "scanned"
                    ? "a scanned PDF: its pages are images, so there's no text to read. Running it through OCR first would fix it."
                    : p.outcome.status === "unsupported" ||
                        p.outcome.status === "failed"
                      ? p.outcome.reason
                      : ""}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}

      {added.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {added.map((a, i) => (
            <span
              key={`${a.name}-${i}`}
              className="rounded-full border border-[var(--line)] px-2 py-0.5 text-[11.5px] text-[var(--ink-dim)]"
            >
              {a.name}
            </span>
          ))}
        </div>
      )}
    </Card>
  );
}

/** A quiet metadata pill for the note header. */
function Meta({
  children,
  mono,
}: {
  children: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <span
      className={cx(
        "rounded-full border border-[var(--line)] px-2 py-0.5 text-[11.5px] text-[var(--ink-dim)]",
        mono && "font-mono",
      )}
    >
      {children}
    </span>
  );
}

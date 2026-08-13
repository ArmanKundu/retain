// The library: your material, and everything Retain has written from it.
//
// Two halves that belong together. **Materials** is what you supply — the study
// design, past papers, your own notes. **Saved** is what the AI produced, kept
// automatically rather than behind a save button you'd have to remember.
//
// The connection between them is the point: a note generated while your study
// design is loaded is grounded in the real document, and the excerpts it used
// are shown so you can check them.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Download,
  FileText,
  Pin,
  Printer,
  Search,
  Sparkle,
  Trash2,
  Upload,
} from "lucide-react";

import { AiAction, AiGate, useAi } from "../components/Ai";
import { Chip, SectionHeader, SubjectPill } from "../components/primitives";
import { Button, Card, Empty, cx } from "../components/ui";
import { api } from "../lib/api";
import type {
  GroundedText,
  LibraryItem,
  LibraryKind,
  Resource,
  ResourceKind,
} from "../lib/types";
import { useApp } from "../store";

const KIND_LABEL: Record<LibraryKind, string> = {
  notes: "Notes",
  practice_question: "Practice",
  weekly_review: "Weekly review",
  answer: "Answer",
  cards: "Cards",
};

const RESOURCE_KINDS: { value: ResourceKind; label: string; hint: string }[] = [
  { value: "study_design", label: "Study design", hint: "What's examinable" },
  { value: "past_paper", label: "Past paper", hint: "How it gets asked" },
  { value: "notes", label: "Notes", hint: "Yours or your teacher's" },
  { value: "other", label: "Other", hint: "Anything else" },
];

export function Library() {
  const [tab, setTab] = useState<"saved" | "materials">("saved");

  return (
    <div className="mx-auto w-full max-w-[min(920px,100%)] px-6 pb-16 sm:px-9">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-6">
        <h1 className="text-[28px] font-semibold tracking-[-0.028em]">Library</h1>
        <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
          Everything Retain has written for you, and the material it writes from.
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
        <SectionHeader title="Write me notes" hint="grounded in your own material" />
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
                onChange={(e) => setSubjectId(e.target.value ? Number(e.target.value) : null)}
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

            {note && <p className="mt-3 text-[12.5px] text-[var(--ink-dim)]">{note}</p>}
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
                {item.pinned && <Pin size={12} className="shrink-0 text-[var(--warn)]" />}

                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13.5px] text-[var(--ink)]">{item.title}</div>
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
                  <SubjectPill name={item.subjectName} colour={item.colour} size="sm" />
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
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        className="glass animate-pop flex max-h-[86vh] w-full max-w-[720px] flex-col rounded-[var(--r-xl)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-3 border-b border-[var(--line-soft)] p-5">
          <div className="min-w-0 flex-1">
            <h2 className="text-[17px] font-semibold tracking-[-0.01em]">{item.title}</h2>
            <div className="mt-1 flex flex-wrap items-center gap-2 text-[11.5px] text-[var(--ink-faint)]">
              <span>{KIND_LABEL[item.kind]}</span>
              <span>·</span>
              <span>{item.createdAt.slice(0, 10)}</span>
              {item.model && (
                <>
                  <span>·</span>
                  <span className="font-mono">{item.model}</span>
                </>
              )}
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
              item.pinned ? "text-[var(--warn)]" : "text-[var(--ink-faint)] hover:text-[var(--ink)]",
            )}
          >
            <Pin size={14} />
          </button>
        </div>

        {/* `print-target` is what the print stylesheet keeps; everything else on
            the page is hidden when printing. */}
        <div className="print-target selectable flex-1 overflow-y-auto p-6">
          {item.prompt && (
            <p className="mb-4 border-l-2 border-[var(--line)] pl-3 text-[12.5px] italic text-[var(--ink-faint)]">
              {item.prompt}
            </p>
          )}
          <pre className="whitespace-pre-wrap font-sans text-[13.5px] leading-[1.7] text-[var(--ink)]">
            {item.body}
          </pre>
        </div>

        <div className="flex flex-wrap items-center gap-2 border-t border-[var(--line-soft)] p-4">
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

          <Button size="sm" onClick={() => window.print()}>
            <Printer size={13} />
            Print
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
  const [hits, setHits] = useState<number | null>(null);

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
          hint={list.length > 0 ? `${totalWords.toLocaleString()} words indexed` : undefined}
        />
        <Uploader subjects={subjects} onAdded={load} onError={setError} />
        {error && <p className="mt-2 text-[12.5px] text-[var(--danger)]">{error}</p>}
      </section>

      {list.length > 0 && (
        <section className="animate-rise mb-8">
          <SectionHeader title="Check coverage" hint="no model call, costs nothing" />
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
                onClick={async () => setHits((await api.searchResources(probe)).length)}
              >
                Check
              </Button>
            </div>
            {hits !== null && (
              <p className="mt-2.5 text-[12.5px] text-[var(--ink-dim)]">
                {hits === 0
                  ? "Nothing in your material matches those words. A generated answer on this would come from the model's own knowledge."
                  : `${hits} passage${hits === 1 ? "" : "s"} match. An answer on this would be grounded in your material.`}
              </p>
            )}
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
            {list.map((r) => (
              <div
                key={r.id}
                className="group flex items-center gap-3 rounded-[var(--r-md)] px-3 py-3 transition-colors duration-[var(--t-fast)] hover:bg-[var(--surface-hi)]/60"
              >
                <FileText size={14} className="shrink-0 text-[var(--ink-faint)]" />

                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13.5px] text-[var(--ink)]">{r.title}</div>
                  <div className="mt-0.5 flex items-center gap-2 text-[11.5px] text-[var(--ink-faint)]">
                    <span>{RESOURCE_KINDS.find((k) => k.value === r.kind)?.label ?? r.kind}</span>
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
        )}
      </section>
    </>
  );
}

/**
 * Add material, by file or by pasting.
 *
 * Text files are read in the webview and only the text crosses to Rust. PDFs
 * can't be read here — the webview has no PDF parser — so the honest thing is
 * to say so and offer paste, rather than accept the file and silently store
 * nothing.
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
  const fileRef = useRef<HTMLInputElement>(null);
  const [title, setTitle] = useState("");
  const [kind, setKind] = useState<ResourceKind>("study_design");
  const [subjectId, setSubjectId] = useState<number | null>(null);
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState(false);

  const commit = async (text: string, source: string | null, fallbackTitle: string) => {
    setBusy(true);
    onError(null);
    try {
      await api.addResource(subjectId, title.trim() || fallbackTitle, kind, source, text);
      setTitle("");
      setPasted("");
      await onAdded();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const readFiles = async (files: FileList) => {
    for (const file of Array.from(files)) {
      if (/\.pdf$/i.test(file.name)) {
        onError(
          `${file.name} is a PDF, which Retain can't read directly. Open it, select all, and paste the text below — or export it as .txt first.`,
        );
        continue;
      }
      const text = await file.text();
      await commit(text, file.name, file.name.replace(/\.[^.]+$/, ""));
    }
  };

  return (
    <Card className="p-5">
      <div className="flex flex-wrap items-center gap-2">
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Title — e.g. VCAA Biology study design 2023–2027"
          className="h-9 min-w-[240px] flex-1 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-3 text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
        />
        <select
          value={subjectId ?? ""}
          onChange={(e) => setSubjectId(e.target.value ? Number(e.target.value) : null)}
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
          <Chip key={k.value} active={kind === k.value} onClick={() => setKind(k.value)} title={k.hint}>
            {k.label}
          </Chip>
        ))}
      </div>

      <textarea
        value={pasted}
        onChange={(e) => setPasted(e.target.value)}
        placeholder="Paste the text here — or use the file button below for .txt and .md files."
        className="selectable mt-3 h-28 w-full resize-none rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] p-3 text-[12.5px] leading-relaxed text-[var(--ink)] placeholder:text-[var(--ink-faint)] outline-none focus:border-[var(--accent)]"
      />

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          disabled={busy || pasted.trim().length < 40}
          onClick={() => void commit(pasted, "pasted", "Pasted material")}
        >
          <Sparkle size={13} />
          {busy ? "Indexing…" : "Add pasted text"}
        </Button>

        <Button size="sm" disabled={busy} onClick={() => fileRef.current?.click()}>
          <Upload size={13} />
          Add a file
        </Button>
        <input
          ref={fileRef}
          type="file"
          multiple
          accept=".txt,.md,.markdown,.csv,.html,.htm,.json"
          className="hidden"
          onChange={(e) => {
            if (e.target.files?.length) void readFiles(e.target.files);
            e.target.value = "";
          }}
        />

        <span className="text-[11.5px] text-[var(--ink-faint)]">
          .txt and .md files. PDFs need pasting for now.
        </span>
      </div>
    </Card>
  );
}

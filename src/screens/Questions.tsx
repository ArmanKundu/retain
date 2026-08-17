// Every past question, searchable.
//
// The library holds over a thousand exam papers. Searching it returned
// *papers*, and a paper is twenty pages — so "show me every calculus question
// in Specialist" meant opening PDFs until you found some.
//
// Papers are cut into questions on the `Question N` marker and indexed
// individually. The tags are your own topic names where they appear in the
// question, plus anything you add — no built-in keyword list, because a list of
// what counts as a topic in VCE Biology is curriculum, and inventing it is the
// one thing Retain doesn't do.

import { useCallback, useEffect, useState } from "react";
import { FileText, Loader2, Search, Tag, X } from "lucide-react";

import { SectionHeader } from "../components/primitives";
import { Button, Card, cx } from "../components/ui";
import { api } from "../lib/api";
import type { PastQuestion, QuestionFacets } from "../lib/types";
import { useApp } from "../store";

export function Questions() {
  const subjects = useApp((s) => s.subjects);

  const [query, setQuery] = useState("");
  const [subjectId, setSubjectId] = useState<number | null>(null);
  const [tag, setTag] = useState<string | null>(null);
  const [facets, setFacets] = useState<QuestionFacets | null>(null);
  const [fromYear, setFromYear] = useState<number | null>(null);
  const [toYear, setToYear] = useState<number | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [includeSolutions, setIncludeSolutions] = useState(false);
  const [open, setOpen] = useState<PastQuestion | null>(null);
  const [results, setResults] = useState<PastQuestion[]>([]);
  const [tags, setTags] = useState<[string, number][]>([]);
  const [indexing, setIndexing] = useState(false);
  const [remaining, setRemaining] = useState<number | null>(null);
  const [total, setTotal] = useState(0);
  const [searched, setSearched] = useState(false);

  const loadTags = useCallback(async () => {
    setTags(await api.questionTags(subjectId).catch(() => []));
  }, [subjectId]);

  useEffect(() => {
    void loadTags();
    void api
      .questionFacets(subjectId)
      .then((f) => {
        setFacets(f);
        // Seed the range from what's actually there. A slider whose ends don't
        // match the data is a control that looks broken the first time you
        // touch it.
        setFromYear(f.minYear);
        setToYear(f.maxYear);
      })
      .catch(() => setFacets(null));
  }, [loadTags, subjectId]);

  // How much is left to index. Costs one query and answers the only question
  // an empty screen raises — is this broken, or have I not built it yet?
  useEffect(() => {
    void api
      .indexQuestions(0)
      .then((p) => {
        setRemaining(p.remaining);
        setTotal(p.questions);
      })
      .catch(() => setRemaining(null));
  }, []);

  // Debounced, because the query runs against FTS on every keystroke.
  useEffect(() => {
    if (!query.trim() && !tag) {
      setResults([]);
      setSearched(false);
      return;
    }
    const t = setTimeout(() => {
      void api
        .searchQuestions(
          query,
          { subjectId, tag, fromYear, toYear, source, includeSolutions },
          60,
        )
        .then((r) => {
          setResults(r);
          setSearched(true);
        })
        .catch(() => setResults([]));
    }, 160);
    return () => clearTimeout(t);
  }, [query, subjectId, tag, fromYear, toYear, source, includeSolutions]);

  /**
   * Work through the backlog in batches.
   *
   * A thousand papers in one call would hold the database lock for seconds and
   * freeze every other screen, so the loop is here and the backend does 25 at
   * a time.
   */
  const runIndex = async () => {
    setIndexing(true);
    try {
      for (;;) {
        const p = await api.indexQuestions(25);
        setRemaining(p.remaining);
        setTotal(p.questions);
        if (p.remaining <= 0 || p.done === 0) break;
      }
      await loadTags();
    } finally {
      setIndexing(false);
    }
  };

  return (
    <div className="mx-auto w-full max-w-[min(1000px,100%)] px-6 pb-16 sm:px-9">
      <div className="titlebar-drag h-11" />

      <header className="animate-rise mb-6">
        <h1 className="text-[28px] font-semibold tracking-[-0.028em]">
          Past questions
        </h1>
        <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
          Every question from every paper you've uploaded, on its own.
        </p>
      </header>

      {/* The backlog. Only shown when there is one — an "index" button with
          nothing to index is a button that does nothing. */}
      {remaining !== null && remaining > 0 && (
        <Card className="animate-rise mb-5 flex flex-wrap items-center gap-3 p-4">
          <div className="min-w-0 flex-1">
            <div className="text-[13.5px] text-[var(--ink)]">
              {total > 0
                ? `${total.toLocaleString()} questions indexed, ${remaining} papers to go`
                : `${remaining} papers haven't been cut into questions yet`}
            </div>
            <p className="mt-0.5 text-[12px] leading-relaxed text-[var(--ink-faint)]">
              Reads the text already stored, so nothing is downloaded or
              uploaded. Safe to stop and pick up later.
            </p>
          </div>
          <Button
            size="sm"
            variant="primary"
            disabled={indexing}
            onClick={() => void runIndex()}
          >
            {indexing ? (
              <Loader2 size={13} className="animate-spin" />
            ) : (
              <FileText size={13} />
            )}
            {indexing ? "Working…" : "Index them"}
          </Button>
        </Card>
      )}

      <Card className="animate-rise mb-4 p-4">
        <div className="flex items-center gap-2.5">
          <Search size={16} className="shrink-0 text-[var(--ink-faint)]" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="calculus, enzyme inhibition, titration…"
            className="min-w-0 flex-1 bg-transparent text-[15px] text-[var(--ink)] outline-none placeholder:text-[var(--ink-faint)]"
          />
          {query && (
            <button
              onClick={() => setQuery("")}
              aria-label="Clear"
              className="pressable rounded-full p-1 text-[var(--ink-faint)] hover:text-[var(--ink)]"
            >
              <X size={13} />
            </button>
          )}
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          <Chip active={subjectId === null} onClick={() => setSubjectId(null)}>
            All subjects
          </Chip>
          {subjects.map((s) => (
            <Chip
              key={s.id}
              active={subjectId === s.id}
              onClick={() => setSubjectId(subjectId === s.id ? null : s.id)}
              colour={s.colour}
            >
              {s.name}
            </Chip>
          ))}
        </div>
      </Card>

      {facets && facets.minYear !== null && (
        <Card className="animate-rise mb-4 flex flex-wrap items-center gap-x-5 gap-y-3 p-4">
          <label className="flex items-center gap-2 text-[12.5px] text-[var(--ink-dim)]">
            Years
            <input
              type="number"
              value={fromYear ?? ""}
              min={facets.minYear ?? undefined}
              max={facets.maxYear ?? undefined}
              onChange={(e) =>
                setFromYear(e.target.value ? Number(e.target.value) : null)
              }
              className="h-7 w-[68px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)] outline-none focus:border-[var(--accent)]"
            />
            <span className="text-[var(--ink-faint)]">to</span>
            <input
              type="number"
              value={toYear ?? ""}
              min={facets.minYear ?? undefined}
              max={facets.maxYear ?? undefined}
              onChange={(e) =>
                setToYear(e.target.value ? Number(e.target.value) : null)
              }
              className="h-7 w-[68px] rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)] outline-none focus:border-[var(--accent)]"
            />
          </label>

          {facets.sources.length > 0 && (
            <label className="flex items-center gap-2 text-[12.5px] text-[var(--ink-dim)]">
              From
              <select
                value={source ?? ""}
                onChange={(e) => setSource(e.target.value || null)}
                className="h-7 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12.5px] text-[var(--ink)] outline-none"
              >
                <option value="">Anyone</option>
                {facets.sources.map((s) => (
                  <option key={s} value={s}>
                    {s.toUpperCase()}
                  </option>
                ))}
              </select>
            </label>
          )}

          {/* Off by default: a topic search wants the questions on it, not the
              answers to them. */}
          <label className="flex items-center gap-2 text-[12.5px] text-[var(--ink-dim)]">
            <input
              type="checkbox"
              checked={includeSolutions}
              onChange={(e) => setIncludeSolutions(e.target.checked)}
            />
            Include answer books
          </label>

          <button
            onClick={() => {
              setFromYear(facets.minYear);
              setToYear(facets.maxYear);
              setSource(null);
              setIncludeSolutions(false);
              setTag(null);
            }}
            className="pressable ml-auto text-[12px] text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            Reset
          </button>
        </Card>
      )}

      {tags.length > 0 && (
        <section className="animate-rise mb-5">
          <SectionHeader title="Topics" hint="from your own topic names" />
          <div className="flex flex-wrap gap-1.5">
            {tags.map(([name, count]) => (
              <Chip
                key={name}
                active={tag === name}
                onClick={() => setTag(tag === name ? null : name)}
              >
                <Tag size={10} className="opacity-60" />
                {name}
                <span className="tabular-nums opacity-50">{count}</span>
              </Chip>
            ))}
          </div>
        </section>
      )}

      {searched && (
        <p className="mb-2 px-1 text-[12.5px] text-[var(--ink-faint)]">
          {results.length === 0
            ? "Nothing matched."
            : `${results.length}${results.length === 60 ? "+" : ""} question${
                results.length === 1 ? "" : "s"
              }`}
        </p>
      )}

      <div className="space-y-2">
        {results.map((q) => (
          <QuestionCard
            key={q.id}
            question={q}
            onTagsChanged={loadTags}
            onOpen={() => setOpen(q)}
          />
        ))}
      </div>

      {open && <QuestionDetail question={open} onClose={() => setOpen(null)} />}

      {!searched && remaining === 0 && total > 0 && (
        <p className="px-1 text-[13.5px] leading-relaxed text-[var(--ink-dim)]">
          {total.toLocaleString()} questions ready. Search for a topic, or pick
          one above.
        </p>
      )}
    </div>
  );
}

function Chip({
  active,
  colour,
  onClick,
  children,
}: {
  active: boolean;
  colour?: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={cx(
        "pressable flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12px] transition-colors duration-[var(--t-fast)]",
        active
          ? "border-[var(--accent)]/40 bg-[var(--accent)]/12 text-[var(--accent)]"
          : "border-[var(--line)] text-[var(--ink-dim)] hover:border-[var(--ink-faint)] hover:text-[var(--ink)]",
      )}
    >
      {colour && !active && (
        <span
          aria-hidden
          className="h-[6px] w-[6px] rounded-full"
          style={{ background: colour }}
        />
      )}
      {children}
    </button>
  );
}

function QuestionCard({
  question,
  onTagsChanged,
  onOpen,
}: {
  question: PastQuestion;
  onTagsChanged: () => Promise<void>;
  onOpen: () => void;
}) {
  const [tags, setTags] = useState(question.tags);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  // Long questions are collapsed: a screen of results where each one is a page
  // is a screen you can't scan.
  const [open, setOpen] = useState(false);

  const long = question.text.length > 340;

  const add = async () => {
    const clean = draft.trim().toLowerCase();
    if (!clean) return;
    setTags((t) => [...new Set([...t, clean])].sort());
    setDraft("");
    setAdding(false);
    await api.tagQuestion(question.id, clean);
    await onTagsChanged();
  };

  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="text-[13px] font-medium text-[var(--ink)]">
          {question.label}
        </span>
        <button
          onClick={onOpen}
          className="pressable min-w-0 truncate text-[11.5px] text-[var(--ink-faint)] hover:text-[var(--accent)]"
        >
          {question.resourceTitle}
        </button>
        {question.year !== null && (
          <span className="shrink-0 rounded-full bg-[var(--surface-hi)] px-2 py-0.5 text-[11px] tabular-nums text-[var(--ink-faint)]">
            {question.year}
          </span>
        )}
        {question.isSolutions && (
          <span className="shrink-0 rounded-full bg-[var(--warn)]/15 px-2 py-0.5 text-[11px] text-[var(--warn)]">
            answer book
          </span>
        )}
        {question.subjectName && (
          <span className="shrink-0 text-[11.5px] text-[var(--ink-faint)]">
            · {question.subjectName}
          </span>
        )}
        <span className="ml-auto shrink-0 text-[11.5px] tabular-nums text-[var(--ink-faint)]">
          {question.words} words
        </span>
      </div>

      <p
        className={cx(
          "selectable mt-2 whitespace-pre-wrap text-[13.5px] leading-relaxed text-[var(--ink-dim)]",
          long && !open && "line-clamp-4",
        )}
      >
        {question.text}
      </p>

      {long && (
        <button
          onClick={() => setOpen((o) => !o)}
          className="pressable mt-1 text-[12px] text-[var(--accent)]"
        >
          {open ? "Show less" : "Show the whole question"}
        </button>
      )}

      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        {tags.map((t) => (
          <span
            key={t}
            className="group flex items-center gap-1 rounded-full border border-[var(--line)] px-2 py-0.5 text-[11.5px] text-[var(--ink-dim)]"
          >
            {t}
            <button
              onClick={async () => {
                setTags((list) => list.filter((x) => x !== t));
                await api.untagQuestion(question.id, t);
                await onTagsChanged();
              }}
              aria-label={`Remove ${t}`}
              className="pressable text-[var(--ink-faint)] opacity-0 hover:text-[var(--danger)] group-hover:opacity-100"
            >
              <X size={10} />
            </button>
          </span>
        ))}

        {adding ? (
          <input
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={() => void add()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void add();
              if (e.key === "Escape") {
                setDraft("");
                setAdding(false);
              }
            }}
            placeholder="tag"
            className="h-[22px] w-24 rounded-full border border-[var(--accent)]/40 bg-transparent px-2 text-[11.5px] text-[var(--ink)] outline-none"
          />
        ) : (
          <button
            onClick={() => setAdding(true)}
            className="pressable rounded-full border border-dashed border-[var(--line)] px-2 py-0.5 text-[11.5px] text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            + tag
          </button>
        )}
      </div>
    </Card>
  );
}

/**
 * One question, with its answers if the library has them.
 *
 * Pairing is exact — `2018 kilbaha exam 1` finds `2018 kilbaha exam 1
 * solutions` and nothing else. Fuzzy matching here would eventually show you
 * the answer to a different question, which you would revise from and never
 * notice.
 */
function QuestionDetail({
  question,
  onClose,
}: {
  question: PastQuestion;
  onClose: () => void;
}) {
  const [solutions, setSolutions] = useState<[number, string] | null>(null);
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    void api
      .questionSolutions(question.resourceId)
      .then(setSolutions)
      .catch(() => setSolutions(null))
      .finally(() => setChecked(true));
  }, [question.resourceId]);

  return (
    <div
      className="fixed inset-0 z-30 flex items-start justify-center overflow-y-auto bg-black/45 p-8 backdrop-blur-sm"
      onClick={onClose}
    >
      <Card
        className="animate-rise w-full max-w-[760px] p-6"
        onClick={(e: React.MouseEvent) => e.stopPropagation()}
      >
        <div className="flex flex-wrap items-baseline gap-2">
          <h2 className="text-[16px] font-semibold tracking-[-0.01em]">
            {question.label}
          </h2>
          <span className="text-[12.5px] text-[var(--ink-dim)]">
            {question.resourceTitle}
          </span>
          <button
            onClick={onClose}
            aria-label="Close"
            className="pressable ml-auto rounded-full p-1 text-[var(--ink-faint)] hover:text-[var(--ink)]"
          >
            <X size={14} />
          </button>
        </div>

        <p className="selectable mt-4 whitespace-pre-wrap text-[13.5px] leading-relaxed text-[var(--ink)]">
          {question.text}
        </p>

        <div className="mt-5 border-t border-[var(--line-soft)] pt-4">
          {!checked ? null : solutions ? (
            <p className="text-[12.5px] leading-relaxed text-[var(--ink-dim)]">
              Answers are in{" "}
              <span className="text-[var(--ink)]">{solutions[1]}</span>. Open it
              from the Library — Retain stores the text, not the original PDF,
              so the marking scheme's layout isn't reproduced here.
            </p>
          ) : (
            <p className="text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
              No answer book for this paper is in your library.
            </p>
          )}
        </div>
      </Card>
    </div>
  );
}

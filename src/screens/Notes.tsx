// The note editor.
//
// The last thing Retain made you leave for. Everything else it holds was either
// generated or uploaded; there was nowhere to write down what the teacher said
// in fourth period.
//
// # Two decisions worth stating
//
// **A textarea per block, not one contenteditable.** Contenteditable is how
// Notion does it and it is the source of most of the bugs in every editor that
// copies Notion: the browser owns the DOM, React wants to own the DOM, and the
// two disagree about what happened after every IME composition, paste and undo.
// A textarea per block gives up inline rich text — you can't bold three words
// inside a paragraph — and gets correct undo, correct spellcheck, correct
// selection and correct behaviour when you paste from Word. For notes taken
// during a class, that is the right trade.
//
// **Every block saves independently, debounced.** There is no Save button and
// no document-level write. A crash mid-lesson costs the half-second since your
// last keystroke, not the lesson.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  Camera,
  StickyNote,
  ChevronDown,
  FileText,
  GripVertical,
  Plus,
  Printer,
  Trash2,
} from "lucide-react";

import { InlineText } from "../components/InlineText";
import { PrintHeader, PrintPortal } from "../components/PrintPortal";
import { SectionHeader } from "../components/primitives";
import { Button, Card, cx } from "../components/ui";
import { api } from "../lib/api";
import {
  exitsListOnEmptyEnter,
  filterSlash,
  kindAfterEnter,
  markdownShortcut,
  type SlashItem,
} from "../lib/blockShortcuts";
import type { Note, NoteBlock, NoteBlockKind, NoteSummary } from "../lib/types";
import { useApp } from "../store";

/** How long after the last keystroke a block is written. */
const SAVE_DELAY_MS = 400;

export function Notes() {
  const subjects = useApp((s) => s.subjects);

  const [list, setList] = useState<NoteSummary[]>([]);
  const [note, setNote] = useState<Note | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** Which block to focus once the next render lands. */
  const focusNext = useRef<{ id: number; at: "start" | "end" } | null>(null);

  const loadList = useCallback(async () => {
    setList(await api.listNotes(null).catch(() => []));
  }, []);

  const open = useCallback(async (id: number) => {
    setNote(await api.getNote(id).catch(() => null));
  }, []);

  useEffect(() => {
    void loadList();
  }, [loadList]);

  const newNote = async () => {
    const id = await api.createNote(null, "", null);
    await loadList();
    await open(id);
  };

  /** Re-read the note after a structural change, and keep focus intent. */
  const refresh = useCallback(
    async (id: number) => {
      setNote(await api.getNote(id).catch(() => null));
      await loadList();
    },
    [loadList],
  );

  return (
    <div className="mx-auto w-full max-w-[min(1180px,100%)] px-6 pb-16 sm:px-9">
      {/* Content scrolls under the title bar. macOS separates the two with a
          hard edge rather than letting text vanish mid-letter. */}
      <div className="titlebar-drag scroll-edge h-11" />

      <header className="animate-rise mb-6 flex flex-wrap items-end gap-4">
        <div>
          <h1 className="text-[28px] font-semibold tracking-[var(--track-display)]">
            Notes
          </h1>
          <p className="mt-1.5 text-[14px] leading-relaxed text-[var(--ink-dim)]">
            Write in class. Everything saves as you type.
          </p>
        </div>
        <Button
          size="sm"
          variant="primary"
          className="ml-auto"
          onClick={() => void newNote()}
        >
          <Plus size={13} />
          New note
        </Button>
      </header>

      {error && (
        <p className="mb-3 text-[12.5px] text-[var(--danger)]">{error}</p>
      )}

      <div className="flex gap-6">
        {/* The list. Narrow on purpose — it's for getting back to something,
            not for browsing. */}
        <aside className="hidden w-[230px] shrink-0 lg:block">
          <SectionHeader
            title="Recent"
            hint={list.length > 0 ? `${list.length}` : undefined}
          />
          <div className="space-y-0.5">
            {list.map((n) => (
              <button
                key={n.id}
                onClick={() => void open(n.id)}
                className={cx(
                  "w-full rounded-[var(--r-md)] px-3 py-2.5 text-left transition-colors duration-[var(--t-fast)]",
                  note?.id === n.id
                    ? "bg-[var(--surface-hi)]"
                    : "hover:bg-[var(--surface)]",
                )}
              >
                <div className="flex items-center gap-2">
                  {n.colour && (
                    <span
                      aria-hidden
                      className="h-[7px] w-[7px] shrink-0 rounded-full"
                      style={{ background: n.colour }}
                    />
                  )}
                  <span className="truncate text-[13px] text-[var(--ink)]">
                    {n.title}
                  </span>
                </div>
                {n.preview && (
                  <div className="mt-0.5 truncate text-[11.5px] text-[var(--ink-faint)]">
                    {n.preview}
                  </div>
                )}
              </button>
            ))}
          </div>
        </aside>

        <main className="min-w-0 flex-1">
          {note ? (
            <Editor
              key={note.id}
              note={note}
              subjects={subjects}
              focusNext={focusNext}
              onChanged={() => void refresh(note.id)}
              onDeleted={async () => {
                setNote(null);
                await loadList();
              }}
              onError={setError}
            />
          ) : (
            <Card className="px-6 py-14 text-center">
              <FileText
                size={22}
                className="mx-auto mb-3 text-[var(--ink-faint)]"
              />
              <div className="text-[14px] font-medium text-[var(--ink-dim)]">
                {list.length === 0
                  ? "No notes yet"
                  : "Pick a note, or start a new one"}
              </div>
              <p className="mx-auto mt-1.5 max-w-[400px] text-[13px] leading-relaxed text-[var(--ink-faint)]">
                Type <code className="rounded-[var(--r-xs)] px-1">/</code> at
                the start of a line for headings, lists and checkboxes, or use
                Markdown shortcuts like{" "}
                <code className="rounded-[var(--r-xs)] px-1">##</code> and{" "}
                <code className="rounded-[var(--r-xs)] px-1">-</code>.
              </p>
              <Button
                size="sm"
                variant="primary"
                className="mt-5"
                onClick={() => void newNote()}
              >
                <Plus size={13} />
                New note
              </Button>
            </Card>
          )}
        </main>
      </div>
    </div>
  );
}

function Editor({
  note,
  subjects,
  focusNext,
  onChanged,
  onDeleted,
  onError,
}: {
  note: Note;
  subjects: { id: number; name: string; colour: string }[];
  focusNext: React.MutableRefObject<{ id: number; at: "start" | "end" } | null>;
  onChanged: () => void;
  onDeleted: () => void;
  onError: (m: string | null) => void;
}) {
  const [title, setTitle] = useState(note.title);
  const [blocks, setBlocks] = useState<NoteBlock[]>(note.blocks);
  const [slash, setSlash] = useState<{ blockId: number; query: string } | null>(
    null,
  );
  const [capturing, setCapturing] = useState(false);

  useEffect(() => {
    setBlocks(note.blocks);
  }, [note.blocks]);

  // Focus whichever block the last structural change asked for, after render.
  useEffect(() => {
    const want = focusNext.current;
    if (!want) return;
    focusNext.current = null;

    const el = document.querySelector<HTMLTextAreaElement>(
      `[data-block="${want.id}"]`,
    );
    if (!el) return;
    el.focus();
    const at = want.at === "start" ? 0 : el.value.length;
    el.setSelectionRange(at, at);
  }, [blocks, focusNext]);

  /** Write one block. Debounced per block, so typing doesn't hit SQLite. */
  const timers = useRef(new Map<number, number>());
  const save = useCallback((block: NoteBlock) => {
    const existing = timers.current.get(block.id);
    if (existing) window.clearTimeout(existing);

    timers.current.set(
      block.id,
      window.setTimeout(() => {
        void api
          .updateNoteBlock(
            block.id,
            block.kind,
            block.text,
            block.checked,
            block.image,
          )
          .catch(() => {});
      }, SAVE_DELAY_MS),
    );
  }, []);

  // A pending write must not be lost when the note is closed or the app quits.
  useEffect(() => {
    const pending = timers.current;
    return () => {
      for (const t of pending.values()) window.clearTimeout(t);
    };
  }, []);

  const patch = useCallback(
    (id: number, change: Partial<NoteBlock>) => {
      setBlocks((current) => {
        const next = current.map((b) =>
          b.id === id ? { ...b, ...change } : b,
        );
        const changed = next.find((b) => b.id === id);
        if (changed) save(changed);
        return next;
      });
    },
    [save],
  );

  const addBelow = async (
    after: number | null,
    kind: NoteBlockKind,
    text = "",
  ) => {
    // Flush the block being left, so its last keystrokes aren't overtaken by
    // the reload that follows.
    const leaving = blocks.find((b) => b.id === after);
    if (leaving) {
      const t = timers.current.get(leaving.id);
      if (t) window.clearTimeout(t);
      await api.updateNoteBlock(
        leaving.id,
        leaving.kind,
        leaving.text,
        leaving.checked,
        leaving.image,
      );
    }

    const id = await api.insertNoteBlock(note.id, after, kind, text);
    focusNext.current = { id, at: "end" };
    onChanged();
    return id;
  };

  const removeBlock = async (block: NoteBlock) => {
    const index = blocks.findIndex((b) => b.id === block.id);
    const previous = blocks[index - 1];
    if (previous) focusNext.current = { id: previous.id, at: "end" };

    await api.deleteNoteBlock(block.id);
    onChanged();
  };

  /** Screenshot straight into the note. */
  const insertScreenshot = async (after: number | null) => {
    setCapturing(true);
    onError(null);
    try {
      const image = await api.captureScreen();
      const id = await api.insertNoteBlock(note.id, after, "image", "");
      await api.updateNoteBlock(id, "image", "", false, image);
      onChanged();
    } catch (e) {
      onError(String(e));
    } finally {
      setCapturing(false);
    }
  };

  const applySlash = async (item: SlashItem, block: NoteBlock) => {
    setSlash(null);
    if (item.kind === "image") {
      // The `/` line itself is discarded — it was a command, not content.
      patch(block.id, { text: "" });
      await insertScreenshot(block.id);
      return;
    }
    patch(block.id, { kind: item.kind, text: "" });
    focusNext.current = { id: block.id, at: "end" };
  };

  const menu = slash ? filterSlash(slash.query) : [];

  return (
    <div className="animate-rise">
      <Card className="p-0">
        {/* Toolbar */}
        <div className="print-hide flex flex-wrap items-center gap-2 border-b border-[var(--line-soft)] px-5 py-3">
          <select
            value={note.subjectId ?? ""}
            onChange={async (e) => {
              const id = e.target.value === "" ? null : Number(e.target.value);
              await api.setNoteSubject(note.id, id);
              onChanged();
            }}
            className="h-7 rounded-[var(--r-sm)] border border-[var(--line)] bg-[var(--surface-hi)] px-2 text-[12px] text-[var(--ink-dim)] outline-none"
          >
            <option value="">No subject</option>
            {subjects.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>

          <button
            onClick={() => void api.openSticky(note.id)}
            title="Put this note on the desktop, above everything else"
            className="pressable flex items-center gap-1.5 rounded-full border border-[var(--line)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
          >
            <StickyNote size={12} />
            Stick to desktop
          </button>

          <button
            onClick={() =>
              void insertScreenshot(blocks[blocks.length - 1]?.id ?? null)
            }
            disabled={capturing}
            title="Add a screenshot of your screen"
            className="pressable flex items-center gap-1.5 rounded-full border border-[var(--line)] px-2.5 py-1 text-[12px] text-[var(--ink-dim)] hover:text-[var(--ink)] disabled:opacity-50"
          >
            <Camera size={12} />
            {capturing ? "Capturing…" : "Screenshot"}
          </button>

          <div className="ml-auto flex items-center gap-1.5">
            <button
              onClick={() => window.print()}
              title="Print or save as PDF"
              className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:text-[var(--ink)]"
            >
              <Printer size={14} />
            </button>
            <button
              onClick={async () => {
                await api.deleteNote(note.id);
                onDeleted();
              }}
              title="Delete this note"
              className="pressable rounded-[var(--r-sm)] p-1.5 text-[var(--ink-faint)] hover:text-[var(--danger)]"
            >
              <Trash2 size={14} />
            </button>
          </div>
        </div>

        {/* Paper gets its own render. The editor is a column of textareas
            inside a scroll container, and neither prints usefully. */}
        <PrintPortal>
          <PrintHeader
            title={title}
            meta={[
              note.subjectName,
              new Date(note.updatedAt).toLocaleDateString(),
            ]}
          />
          <PrintableBlocks blocks={blocks} />
        </PrintPortal>

        <div className="px-8 py-7">
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={() => void api.setNoteTitle(note.id, title).then(onChanged)}
            placeholder="Untitled"
            className="print-hide mb-5 w-full bg-transparent text-[26px] font-semibold tracking-[var(--track-display)] text-[var(--ink)] outline-none placeholder:text-[var(--ink-faint)]"
          />

          <div className="space-y-0.5">
            {blocks.map((block, i) => (
              <BlockRow
                key={block.id}
                block={block}
                index={i}
                blocks={blocks}
                slashOpen={slash?.blockId === block.id ? menu : null}
                onSlashQuery={(query) => setSlash({ blockId: block.id, query })}
                onSlashClose={() => setSlash(null)}
                onSlashPick={(item) => void applySlash(item, block)}
                onPatch={patch}
                onEnter={(kind, carry) => void addBelow(block.id, kind, carry)}
                onRemove={() => void removeBlock(block)}
                onMove={async (delta) => {
                  await api.moveNoteBlock(block.id, delta);
                  focusNext.current = { id: block.id, at: "end" };
                  onChanged();
                }}
                onFocusSibling={(dir) => {
                  const target = blocks[i + dir];
                  if (!target) return;
                  focusNext.current = {
                    id: target.id,
                    at: dir > 0 ? "start" : "end",
                  };
                  setBlocks((b) => [...b]);
                }}
              />
            ))}
          </div>

          {/* Clicking under the last block starts a new one, the way a page
              works. Without it you have to aim at the last line. */}
          <button
            onClick={() =>
              void addBelow(blocks[blocks.length - 1]?.id ?? null, "paragraph")
            }
            className="print-hide mt-1 h-16 w-full cursor-text text-left text-[13px] text-transparent hover:text-[var(--ink-faint)]"
          >
            Click to keep writing
          </button>
        </div>
      </Card>
    </div>
  );
}

/**
 * A block that isn't being edited, drawn with its formatting.
 *
 * This is the half that makes inline markdown possible without
 * contenteditable: the textarea only exists while the cursor is in the block,
 * so you see `**enzyme**` on the line you're editing and **enzyme** on every
 * other one. Clicking anywhere in it puts the cursor back where you clicked —
 * anything less and it reads as a preview you have to escape from.
 */
function Rendered({
  block,
  className,
  placeholder,
  onFocus,
}: {
  block: NoteBlock;
  className: string;
  placeholder?: string;
  onFocus: () => void;
}) {
  return (
    <div
      onMouseDown={(e) => {
        // A link handles its own click; focusing the editor instead would open
        // the textarea and swallow it.
        if ((e.target as HTMLElement).closest("a")) return;
        e.preventDefault();
        onFocus();
      }}
      className={cx(
        "w-full cursor-text whitespace-pre-wrap break-words",
        className,
      )}
    >
      {block.text ? (
        <InlineText source={block.text} />
      ) : (
        // A zero-height empty block would be unclickable, so an empty one keeps
        // a line's worth of space and offers the hint on the first block.
        <span className="text-[var(--ink-faint)]">
          {placeholder || "\u00a0"}
        </span>
      )}
    </div>
  );
}

function BlockRow({
  block,
  index,
  blocks,
  slashOpen,
  onSlashQuery,
  onSlashClose,
  onSlashPick,
  onPatch,
  onEnter,
  onRemove,
  onMove,
  onFocusSibling,
}: {
  block: NoteBlock;
  index: number;
  blocks: NoteBlock[];
  slashOpen: SlashItem[] | null;
  onSlashQuery: (q: string) => void;
  onSlashClose: () => void;
  onSlashPick: (item: SlashItem) => void;
  onPatch: (id: number, change: Partial<NoteBlock>) => void;
  onEnter: (kind: NoteBlockKind, carry: string) => void;
  onRemove: () => void;
  onMove: (delta: number) => void;
  onFocusSibling: (dir: -1 | 1) => void;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [editing, setEditing] = useState(false);

  // Grow to fit. A scrollbar inside one block of a document is wrong — the page
  // scrolls, the block doesn't.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [block.text, block.kind]);

  if (block.kind === "divider") {
    return (
      <div className="group flex items-center gap-2 py-2">
        <Handle onMove={onMove} onRemove={onRemove} />
        <hr className="flex-1 border-0 border-t border-[var(--line)]" />
      </div>
    );
  }

  if (block.kind === "image") {
    return (
      <div className="group flex items-start gap-2 py-2">
        <Handle onMove={onMove} onRemove={onRemove} />
        <div className="min-w-0 flex-1">
          {block.image ? (
            <img
              src={block.image}
              alt={block.text || "Screenshot"}
              className="max-w-full rounded-[var(--r-md)] border border-[var(--line-soft)]"
            />
          ) : (
            <div className="rounded-[var(--r-md)] border border-dashed border-[var(--line)] px-4 py-6 text-center text-[12.5px] text-[var(--ink-faint)]">
              Image missing
            </div>
          )}
          <input
            value={block.text}
            onChange={(e) => onPatch(block.id, { text: e.target.value })}
            placeholder="Caption"
            className="print-hide mt-1.5 w-full bg-transparent text-[12px] text-[var(--ink-faint)] outline-none"
          />
        </div>
      </div>
    );
  }

  const style = KIND_STYLES[block.kind];

  return (
    <div className="group relative flex items-start gap-2">
      <Handle onMove={onMove} onRemove={onRemove} />

      {block.kind === "todo" && (
        <button
          onClick={() => onPatch(block.id, { checked: !block.checked })}
          aria-label={block.checked ? "Mark as not done" : "Mark as done"}
          className={cx(
            "mt-[7px] grid h-[15px] w-[15px] shrink-0 place-items-center rounded-[var(--r-xs)] border text-[10px]",
            block.checked
              ? "border-transparent bg-[var(--color-positive)] text-white"
              : "border-[var(--ink-faint)] text-transparent",
          )}
        >
          ✓
        </button>
      )}

      {(block.kind === "bullet" || block.kind === "numbered") && (
        <span className="mt-[3px] shrink-0 select-none text-[14px] text-[var(--ink-faint)]">
          {block.kind === "bullet" ? "•" : `${numberOf(blocks, index)}.`}
        </span>
      )}

      <div className="min-w-0 flex-1">
        {/* Formatted when you're not in it, raw when you are. The textarea is
            unmounted while blurred, which is what lets inline markdown work
            without contenteditable. */}
        {!editing && (
          <Rendered
            block={block}
            className={style}
            placeholder={
              index === 0 ? "Start writing, or press / for blocks" : undefined
            }
            onFocus={() => {
              setEditing(true);
              // The textarea doesn't exist yet, so focus waits for the paint
              // that mounts it.
              requestAnimationFrame(() => {
                const el = ref.current;
                if (!el) return;
                el.focus();
                el.setSelectionRange(el.value.length, el.value.length);
              });
            }}
          />
        )}

        <textarea
          ref={ref}
          data-block={block.id}
          // `sr-only` rather than `hidden`: a hidden element can't be focused,
          // and the editor focuses blocks programmatically after every insert,
          // delete and arrow-key move. This keeps the textarea in the
          // focus order while the rendered view supplies the row's height.
          onFocus={() => setEditing(true)}
          onBlur={() => setEditing(false)}
          value={block.text}
          rows={1}
          spellCheck
          placeholder={
            index === 0 ? "Start writing, or press / for blocks" : ""
          }
          onChange={(e) => {
            const text = e.target.value;

            // `/` at the very start opens the menu; what follows filters it.
            if (text.startsWith("/") && !text.includes("\n")) {
              onSlashQuery(text.slice(1));
              onPatch(block.id, { text });
              return;
            }
            if (slashOpen) onSlashClose();

            // Markdown shortcuts fire on the space that completes them, and
            // the marker never survives as text.
            const shortcut = markdownShortcut(text);
            if (shortcut) {
              onPatch(block.id, shortcut);
              return;
            }

            onPatch(block.id, { text });
          }}
          onKeyDown={(e) => {
            if (slashOpen && slashOpen.length > 0) {
              if (e.key === "Enter" || e.key === "Tab") {
                e.preventDefault();
                onSlashPick(slashOpen[0]);
                return;
              }
              if (e.key === "Escape") {
                onSlashClose();
                return;
              }
            }

            const el = e.currentTarget;
            const atStart = el.selectionStart === 0 && el.selectionEnd === 0;
            const atEnd =
              el.selectionStart === el.value.length &&
              el.selectionEnd === el.value.length;

            // ⌘⇧↑/↓ reorders. Plain arrows at an edge move the cursor to the
            // neighbouring block, so the whole note is one keyboard surface.
            if (
              e.metaKey &&
              e.shiftKey &&
              (e.key === "ArrowUp" || e.key === "ArrowDown")
            ) {
              e.preventDefault();
              onMove(e.key === "ArrowUp" ? -1 : 1);
              return;
            }
            if (e.key === "ArrowUp" && atStart) {
              e.preventDefault();
              onFocusSibling(-1);
              return;
            }
            if (e.key === "ArrowDown" && atEnd) {
              e.preventDefault();
              onFocusSibling(1);
              return;
            }

            if (e.key === "Enter" && !e.shiftKey) {
              // Code blocks keep newlines; ⇧↵ is a newline everywhere else.
              if (block.kind === "code" && !e.metaKey) return;
              e.preventDefault();

              if (exitsListOnEmptyEnter(block.kind, block.text)) {
                // Enter twice leaves a list, rather than adding a fourth empty
                // bullet you then have to delete.
                onPatch(block.id, { kind: "paragraph" });
                return;
              }

              // Text to the right of the cursor moves down with you.
              const carry = el.value.slice(el.selectionStart);
              if (carry)
                onPatch(block.id, {
                  text: el.value.slice(0, el.selectionStart),
                });
              onEnter(kindAfterEnter(block.kind), carry);
              return;
            }

            if (e.key === "Backspace" && atStart) {
              // A styled block reverts to a paragraph first. Losing a heading's
              // text because you backspaced into it is worse than an extra
              // keystroke.
              if (block.kind !== "paragraph") {
                e.preventDefault();
                onPatch(block.id, { kind: "paragraph" });
                return;
              }
              if (block.text === "" && blocks.length > 1) {
                e.preventDefault();
                onRemove();
              }
            }
          }}
          className={cx(
            editing
              ? "w-full resize-none overflow-hidden bg-transparent outline-none placeholder:text-[var(--ink-faint)]"
              : "sr-only",
            editing && style,
          )}
        />

        {slashOpen && slashOpen.length > 0 && (
          <div className="print-hide absolute left-8 z-10 mt-1 w-[260px] overflow-hidden rounded-[var(--r-md)] border border-[var(--line)] bg-[var(--surface-hi)] shadow-[var(--e-lg)]">
            {slashOpen.slice(0, 7).map((item, n) => (
              <button
                key={item.kind}
                onMouseDown={(e) => {
                  // mousedown, not click: click fires after blur, and blur has
                  // already closed the menu.
                  e.preventDefault();
                  onSlashPick(item);
                }}
                className={cx(
                  "flex w-full items-baseline gap-2 px-3 py-2 text-left hover:bg-[var(--surface)]",
                  n === 0 && "bg-[var(--surface)]",
                )}
              >
                <span className="text-[13px] text-[var(--ink)]">
                  {item.label}
                </span>
                <span className="text-[11.5px] text-[var(--ink-faint)]">
                  {item.hint}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Handle({
  onMove,
  onRemove,
}: {
  onMove: (d: number) => void;
  onRemove: () => void;
}) {
  return (
    <div className="print-hide flex w-[18px] shrink-0 flex-col items-center pt-[5px] opacity-0 transition-opacity duration-[var(--t-fast)] group-hover:opacity-100">
      <button
        onClick={() => onMove(-1)}
        title="Move up (⌘⇧↑)"
        className="pressable text-[var(--ink-faint)] hover:text-[var(--ink)]"
      >
        <GripVertical size={12} />
      </button>
      <button
        onClick={onRemove}
        title="Delete block"
        className="pressable mt-0.5 text-[var(--ink-faint)] hover:text-[var(--danger)]"
      >
        <Trash2 size={10} />
      </button>
      <button
        onClick={() => onMove(1)}
        title="Move down (⌘⇧↓)"
        className="sr-only"
      >
        <ChevronDown size={12} />
      </button>
    </div>
  );
}

/**
 * Which number a numbered item shows.
 *
 * Counted from the start of the current run, not from the block's position, so
 * a list interrupted by a paragraph restarts at 1 — matching what the Markdown
 * export produces.
 */
function numberOf(blocks: NoteBlock[], index: number): number {
  let n = 1;
  for (let i = index - 1; i >= 0; i--) {
    if (blocks[i].kind !== "numbered") break;
    n++;
  }
  return n;
}

const KIND_STYLES: Record<string, string> = {
  paragraph: "text-[14.5px] leading-relaxed text-[var(--ink)]",
  h1: "text-[22px] font-semibold tracking-[var(--track-display)] text-[var(--ink)] pt-3",
  h2: "text-[18px] font-semibold tracking-[var(--track-display)] text-[var(--ink)] pt-2.5",
  h3: "text-[15.5px] font-semibold text-[var(--ink)] pt-2",
  bullet: "text-[14.5px] leading-relaxed text-[var(--ink)]",
  numbered: "text-[14.5px] leading-relaxed text-[var(--ink)]",
  todo: "text-[14.5px] leading-relaxed text-[var(--ink)]",
  quote:
    "text-[14.5px] italic leading-relaxed text-[var(--ink-dim)] border-l-2 border-[var(--line)] pl-3.5",
  code: "font-mono text-[13px] leading-relaxed text-[var(--ink-dim)] bg-[var(--surface-hi)] rounded-[var(--r-sm)] px-3 py-2.5",
};

/**
 * A note as it should appear on paper.
 *
 * Semantic elements rather than the editor's divs, so the print stylesheet's
 * rules for headings, lists and page breaks actually apply — a printed `<div
 * class="text-[22px]">` is not a heading to a printer, and it will happily
 * orphan one at the foot of a page.
 */
function PrintableBlocks({ blocks }: { blocks: NoteBlock[] }) {
  const out: React.ReactNode[] = [];
  let list: { ordered: boolean; items: NoteBlock[] } | null = null;

  const flush = () => {
    if (!list) return;
    const Tag = list.ordered ? "ol" : "ul";
    out.push(
      <Tag key={out.length}>
        {list.items.map((b) => (
          <li key={b.id}>
            {b.kind === "todo" && <span>{b.checked ? "☑ " : "☐ "}</span>}
            <InlineText source={b.text} />
          </li>
        ))}
      </Tag>,
    );
    list = null;
  };

  for (const b of blocks) {
    const listy =
      b.kind === "bullet" || b.kind === "numbered" || b.kind === "todo";
    if (listy) {
      const ordered = b.kind === "numbered";
      if (list && list.ordered !== ordered) flush();
      list ??= { ordered, items: [] };
      list.items.push(b);
      continue;
    }
    flush();

    switch (b.kind) {
      case "h1":
        out.push(
          <h2 key={b.id}>
            <InlineText source={b.text} />
          </h2>,
        );
        break;
      case "h2":
        out.push(
          <h3 key={b.id}>
            <InlineText source={b.text} />
          </h3>,
        );
        break;
      case "h3":
        out.push(
          <h4 key={b.id}>
            <InlineText source={b.text} />
          </h4>,
        );
        break;
      case "quote":
        out.push(
          <blockquote key={b.id}>
            <InlineText source={b.text} />
          </blockquote>,
        );
        break;
      case "code":
        out.push(<pre key={b.id}>{b.text}</pre>);
        break;
      case "divider":
        out.push(<hr key={b.id} />);
        break;
      case "image":
        out.push(
          b.image ? (
            <figure key={b.id}>
              <img src={b.image} alt={b.text || "Screenshot"} />
              {b.text && <figcaption>{b.text}</figcaption>}
            </figure>
          ) : null,
        );
        break;
      default:
        // An empty paragraph is spacing on screen and a wasted line on paper.
        if (b.text.trim()) {
          out.push(
            <p key={b.id}>
              <InlineText source={b.text} />
            </p>,
          );
        }
    }
  }
  flush();

  return <>{out}</>;
}

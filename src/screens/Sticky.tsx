// A sticky note on the desktop.
//
// The same note, the same blocks, the same editor and the same Markdown as the
// Notes screen — just drawn in a small always-on-top window that remembers
// where you put it. Making stickies a separate kind of object would have cost
// two editors and a note you couldn't promote into a real one when it turned
// out to matter.
//
// Deliberately quiet: no title bar, no toolbar until you hover, no chrome
// competing with two lines of homework. macOS's own Stickies is a yellow
// rectangle with a system font and a close box, and that is most of what makes
// it unpleasant to look at all day.

import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Check, Plus, X } from "lucide-react";

import { InlineText } from "../components/InlineText";
import { cx } from "../components/ui";
import { api } from "../lib/api";
import { kindAfterEnter, markdownShortcut } from "../lib/blockShortcuts";
import type { Note, NoteBlock } from "../lib/types";

/**
 * Paper colours, read from the tokens in `index.css`.
 *
 * Each carries its own ink: a single ink colour can't stay legible across six
 * papers, and a sticky you have to squint at is one you stop writing on. The
 * palette never follows the app theme — see the note beside the tokens.
 */
const PAPER_NAMES = ["amber", "rose", "mint", "sky", "lilac", "slate"] as const;

/** The token stem for a colour name, falling back rather than producing
 *  `var(--paper-undefined)` if a stored value ever goes bad. */
function paperKey(name: string): string {
  return (PAPER_NAMES as readonly string[]).includes(name) ? name : "amber";
}

function paperOf(name: string) {
  const key = paperKey(name);
  return {
    bg: `rgb(var(--paper-${key}) / 0.93)`,
    edge: `rgb(var(--paper-${key}-ink) / 0.28)`,
    ink: `rgb(var(--paper-${key}-ink))`,
  };
}

const SAVE_DELAY_MS = 400;

export function Sticky({ noteId }: { noteId: number }) {
  const [note, setNote] = useState<Note | null>(null);
  const [colour, setColour] = useState("amber");
  const [blocks, setBlocks] = useState<NoteBlock[]>([]);
  const [picking, setPicking] = useState(false);
  const focusNext = useRef<number | null>(null);

  const load = useCallback(async () => {
    const [n, s] = await Promise.all([
      api.getNote(noteId).catch(() => null),
      api.getSticky(noteId).catch(() => null),
    ]);
    if (n) {
      setNote(n);
      setBlocks(n.blocks);
    }
    if (s) setColour(s.colour);
  }, [noteId]);

  useEffect(() => {
    void load();
  }, [load]);

  // Remember where the window ends up. Saved on move and resize rather than on
  // close, because a sticky is usually still on screen when the app quits —
  // waiting for a close event would lose every position on every restart.
  useEffect(() => {
    const win = getCurrentWindow();
    let timer: number | undefined;

    const remember = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(async () => {
        try {
          const scale = await win.scaleFactor();
          const pos = (await win.outerPosition()).toLogical(scale);
          const size = (await win.innerSize()).toLogical(scale);
          await api.saveStickyGeometry(
            noteId,
            pos.x,
            pos.y,
            size.width,
            size.height,
          );
        } catch {
          // A position we couldn't save is not worth interrupting anyone over.
        }
      }, 500);
    };

    const moved = win.onMoved(remember);
    const resized = win.onResized(remember);
    return () => {
      window.clearTimeout(timer);
      void moved.then((f) => f());
      void resized.then((f) => f());
    };
  }, [noteId]);

  const timers = useRef(new Map<number, number>());
  const patch = useCallback((id: number, change: Partial<NoteBlock>) => {
    setBlocks((current) => {
      const next = current.map((b) => (b.id === id ? { ...b, ...change } : b));
      const changed = next.find((b) => b.id === id);
      if (changed) {
        const existing = timers.current.get(id);
        if (existing) window.clearTimeout(existing);
        timers.current.set(
          id,
          window.setTimeout(() => {
            void api
              .updateNoteBlock(
                id,
                changed.kind,
                changed.text,
                changed.checked,
                changed.image,
              )
              .catch(() => {});
          }, SAVE_DELAY_MS),
        );
      }
      return next;
    });
  }, []);

  // Focus whatever the last structural change asked for.
  useEffect(() => {
    const want = focusNext.current;
    if (want == null) return;
    focusNext.current = null;
    const el = document.querySelector<HTMLTextAreaElement>(
      `[data-sticky-block="${want}"]`,
    );
    el?.focus();
    el?.setSelectionRange(el.value.length, el.value.length);
  }, [blocks]);

  const addBelow = async (after: number, kind: NoteBlock["kind"]) => {
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
    const id = await api.insertNoteBlock(noteId, after, kind, "");
    focusNext.current = id;
    await load();
  };

  const paper = paperOf(colour);
  const key = paperKey(colour);

  if (!note) return null;

  return (
    // The window is transparent, so this element is the sticky. The whole
    // surface drags except the text — a sticky you can only move by a title bar
    // is one that needs a title bar.
    <div
      data-tauri-drag-region
      className="group flex h-screen w-screen flex-col overflow-hidden rounded-[var(--r-md)]"
      style={{
        // A flat fill reads as a coloured rectangle. Real paper is lit from
        // above, so the top is a touch brighter and the bottom a touch deeper —
        // barely perceptible on its own, and the whole difference between an
        // object sitting on the desktop and a div.
        background: `linear-gradient(178deg,
          rgb(var(--paper-${key}) / 0.97) 0%,
          rgb(var(--paper-${key}) / 0.93) 45%,
          rgb(var(--paper-${key}) / 0.90) 100%)`,
        color: paper.ink,
        border: `0.5px solid ${paper.edge}`,
        boxShadow: `
          0 1px 2px rgba(0,0,0,0.14),
          0 10px 28px -6px rgba(0,0,0,0.30),
          inset 0 1px 0 rgba(255,255,255,0.55),
          inset 0 -1px 0 rgba(0,0,0,0.05)`,
      }}
    >
      {/* A real header strip, present at rest.
          The previous version showed nothing until you hovered, which made a
          sticky read as a blank coloured rectangle — the exact thing that makes
          Apple's look cheap. The strip is a shade of the paper rather than a
          separate colour, so it belongs to the note instead of sitting on it,
          and it doubles as the drag handle and the resize-safe top edge. */}
      <div
        data-tauri-drag-region
        className="flex h-[26px] shrink-0 items-center gap-1.5 px-2.5"
        style={{
          background: `rgb(var(--paper-${key}-ink) / 0.055)`,
          borderBottom: `0.5px solid rgb(var(--paper-${key}-ink) / 0.10)`,
        }}
      >
        <button
          onClick={() => setPicking((p) => !p)}
          title="Paper colour"
          aria-label="Paper colour"
          className="h-[9px] w-[9px] shrink-0 rounded-full transition-transform duration-[var(--t-fast)] hover:scale-125"
          style={{
            background: `rgb(var(--paper-${key}-ink) / 0.42)`,
            boxShadow: "inset 0 0.5px 0 rgba(255,255,255,0.4)",
          }}
        />

        {/* The first line, echoed. A pile of stickies with no titles is a pile
            you have to read one by one. */}
        <span
          data-tauri-drag-region
          className="min-w-0 flex-1 truncate text-[11px] font-medium tracking-[-0.005em]"
          style={{ color: `rgb(var(--paper-${key}-ink) / 0.62)` }}
        >
          {blocks.find((b) => b.text.trim())?.text.trim() || "New note"}
        </span>

        {/* Quiet until hover — these are for when you already want them. */}
        <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-[var(--t-fast)] group-hover:opacity-100">
          <button
            onClick={() =>
              void addBelow(blocks[blocks.length - 1].id, "paragraph")
            }
            title="Add a line"
            aria-label="Add a line"
            className="rounded-[var(--r-xs)] p-[3px] hover:bg-black/10"
            style={{ color: `rgb(var(--paper-${key}-ink) / 0.55)` }}
          >
            <Plus size={11} />
          </button>
          <button
            onClick={() => void api.closeSticky(noteId)}
            title="Put it away — the note is kept"
            aria-label="Close"
            className="rounded-[var(--r-xs)] p-[3px] hover:bg-black/10"
            style={{ color: `rgb(var(--paper-${key}-ink) / 0.55)` }}
          >
            <X size={11} />
          </button>
        </div>
      </div>

      {picking && (
        <div
          className="flex shrink-0 gap-2 px-2.5 py-2"
          style={{
            borderBottom: `0.5px solid rgb(var(--paper-${key}-ink) / 0.10)`,
          }}
        >
          {PAPER_NAMES.map((name) => (
            <button
              key={name}
              onClick={async () => {
                setColour(name);
                setPicking(false);
                await api.setStickyColour(noteId, name);
              }}
              aria-label={name}
              className={cx(
                "h-[15px] w-[15px] rounded-full border transition-transform duration-[var(--t-fast)] hover:scale-110",
                name === colour && "ring-[1.5px] ring-offset-1",
              )}
              style={{
                background: paperOf(name).bg,
                borderColor: paperOf(name).edge,
                ...(name === colour
                  ? ({
                      "--tw-ring-color": paper.ink,
                      "--tw-ring-offset-color": "transparent",
                    } as React.CSSProperties)
                  : {}),
              }}
            />
          ))}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2.5">
        {blocks.map((block, i) => (
          <StickyLine
            key={block.id}
            block={block}
            ink={paper.ink}
            first={i === 0}
            onPatch={patch}
            onEnter={() => void addBelow(block.id, kindAfterEnter(block.kind))}
            onRemove={async () => {
              if (blocks.length <= 1) return;
              const previous = blocks[i - 1];
              if (previous) focusNext.current = previous.id;
              await api.deleteNoteBlock(block.id);
              await load();
            }}
          />
        ))}
      </div>
    </div>
  );
}

function StickyLine({
  block,
  ink,
  first,
  onPatch,
  onEnter,
  onRemove,
}: {
  block: NoteBlock;
  ink: string;
  first: boolean;
  onPatch: (id: number, change: Partial<NoteBlock>) => void;
  onEnter: () => void;
  onRemove: () => void;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [block.text, editing]);

  const size =
    block.kind === "h1" || block.kind === "h2"
      ? "text-[14px] font-semibold"
      : "text-[13px]";
  const shared = cx(
    "w-full leading-[1.5]",
    size,
    block.checked && "line-through opacity-55",
  );

  return (
    <div className="flex items-start gap-1.5 py-[1px]">
      {block.kind === "todo" && (
        <button
          onClick={() => onPatch(block.id, { checked: !block.checked })}
          aria-label={block.checked ? "Not done" : "Done"}
          className="mt-[3px] grid h-[13px] w-[13px] shrink-0 place-items-center rounded-[var(--r-xs)] border"
          style={{
            borderColor: ink,
            background: block.checked ? ink : "transparent",
            color: block.checked ? "#fff" : "transparent",
          }}
        >
          <Check size={8} strokeWidth={3.5} />
        </button>
      )}
      {block.kind === "bullet" && (
        <span className="mt-[1px] shrink-0 select-none text-[13px] opacity-60">
          •
        </span>
      )}

      <div className="min-w-0 flex-1">
        {!editing && (
          <div
            onMouseDown={(e) => {
              if ((e.target as HTMLElement).closest("a")) return;
              e.preventDefault();
              setEditing(true);
              requestAnimationFrame(() => {
                const el = ref.current;
                if (!el) return;
                el.focus();
                el.setSelectionRange(el.value.length, el.value.length);
              });
            }}
            className={cx(
              shared,
              "cursor-text whitespace-pre-wrap break-words",
            )}
          >
            {block.text ? (
              <InlineText source={block.text} />
            ) : (
              <span className="opacity-45">
                {first ? "Write something…" : " "}
              </span>
            )}
          </div>
        )}

        <textarea
          ref={ref}
          data-sticky-block={block.id}
          value={block.text}
          rows={1}
          onFocus={() => setEditing(true)}
          onBlur={() => setEditing(false)}
          onChange={(e) => {
            // Same shortcuts as the full editor — `- `, `[] `, `## ` — so the
            // two never disagree about what typing a marker does.
            const shortcut = markdownShortcut(e.target.value);
            onPatch(block.id, shortcut ?? { text: e.target.value });
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              onEnter();
              return;
            }
            const el = e.currentTarget;
            if (
              e.key === "Backspace" &&
              el.selectionStart === 0 &&
              block.text === ""
            ) {
              e.preventDefault();
              onRemove();
            }
          }}
          className={cx(
            editing
              ? cx(
                  shared,
                  "resize-none overflow-hidden bg-transparent outline-none",
                )
              : "sr-only",
          )}
          style={editing ? { color: ink } : undefined}
        />
      </div>
    </div>
  );
}

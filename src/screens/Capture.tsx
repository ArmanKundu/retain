import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  CornerDownLeft,
  FileText,
  Paperclip,
  Play,
  Search,
  Sparkle,
  Square,
  X,
} from "lucide-react";

import { cx } from "../components/ui";

import { api } from "../lib/api";
import type {
  Excerpt,
  NewCaptureAttachment,
  ParsedCapture,
} from "../lib/types";

/**
 * The ⌘⇧Space bar.
 *
 * It used to do one thing: write a note. Now it is the way into Retain from
 * wherever you happen to be — mid-lecture in Chrome, halfway through a PDF —
 * because the point of a global hotkey is not having to go and find the app
 * first.
 *
 * Three modes, switched with ⇥ or the circles beside the field:
 *
 *   Capture   write it down. The default, and still the fast path.
 *   Find      search everything you've uploaded, without opening the Library.
 *   Timer     start or stop a session.
 *
 * Shaped by the same latency target as before: under three seconds, end to end.
 * The window is created hidden at launch and never destroyed, so the hotkey only
 * shows it. There is no entry animation — an animation you wait for is latency
 * wearing a costume.
 */

type Mode = "capture" | "find" | "timer";

const MODES: { value: Mode; icon: typeof Sparkle; label: string }[] = [
  { value: "capture", icon: Sparkle, label: "Capture" },
  { value: "find", icon: Search, label: "Find" },
  { value: "timer", icon: Play, label: "Timer" },
];

const PLACEHOLDER: Record<Mode, string> = {
  capture: "Capture something…",
  find: "Search everything you've uploaded…",
  timer: "Which subject?",
};

export function Capture() {
  const [mode, setMode] = useState<Mode>("capture");
  const [text, setText] = useState("");
  const [parsed, setParsed] = useState<ParsedCapture | null>(null);
  const [saving, setSaving] = useState(false);
  const [hits, setHits] = useState<Excerpt[]>([]);
  const [subjects, setSubjects] = useState<
    { id: number; name: string; colour: string }[]
  >([]);
  const [running, setRunning] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // Screenshots and files ride along with the note. In class you have four
  // seconds — a photo of the board beats typing "the thing about enzymes".
  const [attachments, setAttachments] = useState<NewCaptureAttachment[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  // This window shares the bundle but not the app's store, so it never ran
  // `applyTheme` — which meant a light-mode app showed a dark capture bar.
  // Read the setting directly: one cheap SQLite lookup, no store, no bootstrap.
  const syncTheme = useCallback(() => {
    const apply = (theme: string) => {
      document.documentElement.dataset.theme =
        theme === "light" ? "light" : "dark";
    };
    void api
      .getSetting("theme")
      .then((t) => apply(t ?? "dark"))
      .catch(() => apply("dark"));
  }, []);

  /** Read what the bar needs to be useful the instant it appears. */
  const refresh = useCallback(() => {
    void api
      .listSubjects(false)
      .then((list) =>
        setSubjects(
          list.map((s) => ({ id: s.id, name: s.name, colour: s.colour })),
        ),
      )
      .catch(() => setSubjects([]));
    void api
      .getTimer()
      .then((t) => setRunning(t ? t.subjectName : null))
      .catch(() => setRunning(null));
  }, []);

  // Refocus every time the window is shown — it stays alive between uses, so
  // without this the second invocation lands on a stale focus state.
  useEffect(() => {
    const un = listen("capture:opened", () => {
      setText("");
      setParsed(null);
      setAttachments([]);
      setHits([]);
      setNote(null);
      setMode("capture");
      syncTheme();
      refresh();
      requestAnimationFrame(() => inputRef.current?.focus());
    });
    syncTheme();
    refresh();
    inputRef.current?.focus();
    return () => {
      void un.then((f) => f());
    };
  }, [syncTheme, refresh]);

  // Debounced work per mode. Capture parses for the hint line; Find searches.
  // Both are advisory — the save path re-parses in Rust, so a slow or skipped
  // preview can never affect what is stored.
  useEffect(() => {
    const value = text.trim();
    if (!value) {
      setParsed(null);
      setHits([]);
      return;
    }

    const t = setTimeout(() => {
      if (mode === "capture") {
        void api
          .saveCapturePreview(value)
          .then(setParsed)
          .catch(() => setParsed(null));
      } else if (mode === "find") {
        void api
          .searchResources(value, null, 6)
          .then(setHits)
          .catch(() => setHits([]));
      }
    }, 120);
    return () => clearTimeout(t);
  }, [text, mode]);

  const dismiss = useCallback(() => {
    setText("");
    setParsed(null);
    void api.hideCaptureWindow();
  }, []);

  const save = useCallback(async () => {
    const value = text.trim();
    // An attachment alone is a valid capture — a screenshot of the board says
    // plenty without a note.
    if ((!value && attachments.length === 0) || saving) return;

    setSaving(true);
    try {
      await api.saveCaptureWithAttachments(
        value || "(screenshot)",
        attachments,
      );
      setText("");
      setParsed(null);
      setAttachments([]);
      await api.hideCaptureWindow();
    } finally {
      setSaving(false);
    }
  }, [text, saving, attachments]);

  /** Start or stop a session without going to the app. */
  const toggleTimer = useCallback(
    async (subjectId?: number) => {
      try {
        if (running) {
          await api.stopTimer();
          setRunning(null);
          setNote("Stopped. Log it from the app when you're back.");
          return;
        }
        if (subjectId == null) return;
        await api.startTimer({
          subjectId,
          topicId: null,
          // Open-ended, matching the app's own start button: you pick the
          // subject, the clock runs, and what it was for is asked at the end.
          mode: "stopwatch",
          workMinutes: null,
          breakMinutes: null,
        });
        await api.hideCaptureWindow();
      } catch (e) {
        setNote(String(e));
      }
    },
    [running],
  );

  const submit = useCallback(() => {
    if (mode === "capture") {
      void save();
    } else if (mode === "timer" && running) {
      void toggleTimer();
    }
  }, [mode, save, running, toggleTimer]);

  /** Pasted images become attachments; pasted text lands in the field. */
  const onPaste = useCallback((e: React.ClipboardEvent) => {
    const image = Array.from(e.clipboardData.files).find((f) =>
      f.type.startsWith("image/"),
    );
    if (!image) return;

    e.preventDefault();
    const reader = new FileReader();
    reader.onload = () =>
      setAttachments((list) => [
        ...list,
        {
          name: image.name || "Screenshot",
          imageDataUrl: String(reader.result),
          text: null,
        },
      ]);
    reader.readAsDataURL(image);
  }, []);

  /** Attach a document. Its text is extracted in Rust, same as the library. */
  const attachFile = useCallback(async () => {
    const picked = await openDialog({
      multiple: true,
      title: "Attach to this capture",
      filters: [
        {
          name: "Documents",
          extensions: ["pdf", "txt", "md", "png", "jpg", "jpeg"],
        },
      ],
    });
    if (!picked) return;

    for (const path of Array.isArray(picked) ? picked : [picked]) {
      const outcome = await api.readFileText(path);
      if (outcome.status === "extracted") {
        setAttachments((list) => [
          ...list,
          { name: outcome.name, imageDataUrl: null, text: outcome.text },
        ]);
      }
    }
  }, []);

  const hasHint =
    mode === "capture" && !!parsed && (!!parsed.subjectName || !!parsed.dueOn);
  const showResults = mode === "find" && hits.length > 0;
  const showSubjects = mode === "timer" && !running;

  return (
    // The window is transparent and undecorated, so these elements *are* the
    // visible object. Top-aligned rather than centred: the window is tall enough
    // to hold results, and the bar has to stay put as they appear beneath it.
    <div
      data-tauri-drag-region
      className="flex h-screen w-screen flex-col items-center px-3 pt-3"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          dismiss();
        }
        if (e.key === "Enter") {
          e.preventDefault();
          submit();
        }
        // Tab cycles modes, so the whole bar is reachable without the mouse.
        if (e.key === "Tab") {
          e.preventDefault();
          const i = MODES.findIndex((m) => m.value === mode);
          setMode(
            MODES[(i + (e.shiftKey ? MODES.length - 1 : 1)) % MODES.length]
              .value,
          );
          setHits([]);
          setNote(null);
        }
      }}
      onPaste={onPaste}
    >
      <div data-tauri-drag-region className="flex w-full items-center gap-2.5">
        {/* The field. One tall pill, fully rounded — Apple's proportions, where
            the bar is a single object rather than a panel with a box in it. */}
        <div
          data-tauri-drag-region
          className="spotlight spotlight-drag flex h-[58px] min-w-0 flex-1 items-center gap-3.5 rounded-full px-5"
        >
          {mode === "find" ? (
            <Search
              size={20}
              strokeWidth={2}
              className="shrink-0 text-[var(--ink-dim)]"
            />
          ) : mode === "timer" ? (
            <Play
              size={19}
              strokeWidth={2}
              className="shrink-0 text-[var(--ink-dim)]"
            />
          ) : (
            <Sparkle
              size={20}
              strokeWidth={1.9}
              className="shrink-0 text-[var(--ink-dim)]"
            />
          )}

          <input
            ref={inputRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={
              running && mode === "timer"
                ? `${running} — running`
                : PLACEHOLDER[mode]
            }
            spellCheck={false}
            autoComplete="off"
            aria-label={MODES.find((m) => m.value === mode)?.label}
            className="min-w-0 flex-1 bg-transparent text-[21px] leading-[1.3] tracking-[-0.015em] text-[var(--ink)] outline-none placeholder:text-[var(--ink-faint)]"
          />

          {mode === "capture" && (
            <button
              onClick={() => void attachFile()}
              title="Attach a screenshot or file"
              aria-label="Attach a screenshot or file"
              className="pressable shrink-0 rounded-full p-1.5 text-[var(--ink-faint)] hover:bg-white/10 hover:text-[var(--ink)]"
            >
              <Paperclip size={16} />
            </button>
          )}

          <kbd
            className={cx(
              "flex h-[24px] shrink-0 items-center rounded-full border px-2",
              "font-mono text-[11px] transition-opacity duration-[var(--t-base)]",
              text.trim() ||
                attachments.length > 0 ||
                (mode === "timer" && running)
                ? "border-white/15 bg-white/10 text-[var(--ink-dim)] opacity-100"
                : "pointer-events-none border-transparent opacity-0",
            )}
          >
            <CornerDownLeft size={11} />
          </kbd>
        </div>

        {/* Modes, as separate circles beside the field rather than crowded
            inside it — otherwise a bar this wide reads as a toolbar. */}
        {MODES.map((m) => (
          <button
            key={m.value}
            onClick={() => {
              setMode(m.value);
              setHits([]);
              setNote(null);
              inputRef.current?.focus();
            }}
            aria-pressed={mode === m.value}
            title={m.label}
            aria-label={m.label}
            className="spotlight-orb pressable grid h-[52px] w-[52px] shrink-0 place-items-center rounded-full text-[var(--ink-dim)]"
          >
            <m.icon size={19} strokeWidth={1.9} />
          </button>
        ))}
      </div>

      {/* Everything below the bar. Results push down from it rather than
          sitting in reserved empty space, so at rest the window is one pill on
          your wallpaper and nothing else. */}
      <div className="mt-2.5 w-full">
        {attachments.length > 0 && (
          <Panel>
            <div className="flex flex-wrap gap-1.5">
              {attachments.map((a, i) => (
                <span
                  key={`${a.name}-${i}`}
                  className="flex items-center gap-1.5 rounded-full border border-white/12 bg-white/8 px-2.5 py-1 text-[12px] text-[var(--ink-dim)]"
                >
                  {a.imageDataUrl ? (
                    <img
                      src={a.imageDataUrl}
                      alt=""
                      className="h-4 w-4 rounded-[3px] object-cover"
                    />
                  ) : (
                    <Paperclip size={11} />
                  )}
                  <span className="max-w-[160px] truncate">{a.name}</span>
                  <button
                    onClick={() =>
                      setAttachments((list) => list.filter((_, j) => j !== i))
                    }
                    aria-label={`Remove ${a.name}`}
                    className="pressable text-[var(--ink-faint)] hover:text-[var(--ink)]"
                  >
                    <X size={11} />
                  </button>
                </span>
              ))}
            </div>
          </Panel>
        )}

        {hasHint && (
          <Panel>
            <div className="flex min-w-0 items-center gap-2 text-[13px]">
              {parsed!.subjectName && (
                <span className="shrink-0 rounded-full bg-[var(--accent)]/20 px-2.5 py-0.5 text-[var(--accent)]">
                  {parsed!.subjectName}
                </span>
              )}
              {parsed!.dueOn && (
                <span className="shrink-0 rounded-full bg-[var(--color-positive)]/20 px-2.5 py-0.5 text-[var(--color-positive)]">
                  due {parsed!.dueOn}
                </span>
              )}
              <span className="truncate text-[var(--ink-dim)]">
                {parsed!.title}
              </span>
            </div>
          </Panel>
        )}

        {showResults && (
          <Panel>
            <ul className="space-y-0.5">
              {hits.map((h, i) => (
                <li
                  key={`${h.resourceId}-${i}`}
                  className="rounded-[var(--r-md)] px-2.5 py-2 hover:bg-white/8"
                >
                  <div className="flex items-center gap-2 text-[12px] text-[var(--ink-faint)]">
                    <FileText size={11} />
                    <span className="truncate">{h.resourceTitle}</span>
                  </div>
                  <p className="mt-0.5 line-clamp-2 text-[13px] leading-relaxed text-[var(--ink-dim)]">
                    {h.content}
                  </p>
                </li>
              ))}
            </ul>
          </Panel>
        )}

        {showSubjects && subjects.length > 0 && (
          <Panel>
            <div className="flex flex-wrap gap-1.5">
              {subjects
                .filter(
                  (s) =>
                    !text.trim() ||
                    s.name.toLowerCase().includes(text.trim().toLowerCase()),
                )
                .map((s) => (
                  <button
                    key={s.id}
                    onClick={() => void toggleTimer(s.id)}
                    className="pressable rounded-full border border-white/12 px-3 py-1.5 text-[13px] text-[var(--ink-dim)] hover:text-[var(--ink)]"
                  >
                    <span
                      aria-hidden
                      className="mr-2 inline-block h-[7px] w-[7px] rounded-full align-middle"
                      style={{ background: s.colour }}
                    />
                    {s.name}
                  </button>
                ))}
            </div>
          </Panel>
        )}

        {mode === "timer" && running && (
          <Panel>
            <button
              onClick={() => void toggleTimer()}
              className="pressable flex items-center gap-2 text-[13.5px] text-[var(--ink)]"
            >
              <Square size={13} className="text-[var(--danger)]" />
              Stop {running}
            </button>
          </Panel>
        )}

        {note && (
          <Panel>
            <p className="text-[13px] leading-relaxed text-[var(--ink-dim)]">
              {note}
            </p>
          </Panel>
        )}
      </div>
    </div>
  );
}

/** One result surface. Same material as the bar, squarer corners. */
function Panel({ children }: { children: React.ReactNode }) {
  return (
    <div className="spotlight animate-rise mb-2 rounded-[var(--r-lg)] px-3.5 py-3">
      {children}
    </div>
  );
}

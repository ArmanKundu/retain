import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CornerDownLeft, Paperclip, Sparkle, X } from "lucide-react";

import { cx } from "../components/ui";

import { api } from "../lib/api";
import type { NewCaptureAttachment, ParsedCapture } from "../lib/types";

/**
 * The ⌘⇧Space capture bar.
 *
 * Target from the brief: under three seconds, end to end. Everything here is
 * shaped by that.
 *
 *   * The window is created hidden at launch and never destroyed, so the hotkey
 *     only calls `show()` + `set_focus()` — no webview boot, no construction.
 *   * There is no fade-in and no entry animation. An animation you wait for is
 *     latency wearing a costume.
 *   * Enter saves and hides in one keystroke. Escape hides without saving.
 *   * Parsing is a live *hint*, not a gate — you never have to look at it, and
 *     nothing waits on it.
 */
export function Capture() {
  const [text, setText] = useState("");
  const [parsed, setParsed] = useState<ParsedCapture | null>(null);
  const [saving, setSaving] = useState(false);
  // Screenshots and files ride along with the note. In class you have four
  // seconds — a photo of the board beats typing "the thing about enzymes".
  const [attachments, setAttachments] = useState<NewCaptureAttachment[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  // This window shares the bundle but not the app's store, so it never ran
  // `applyTheme` — which meant a light-mode app showed a dark capture bar.
  // Read the setting directly: one cheap SQLite lookup, no store, no bootstrap.
  // Set inline rather than via the store's `applyTheme`: importing that module
  // would pull zustand and the whole app store into this window, which is
  // exactly what `main.tsx` keeps out of the capture path.
  const syncTheme = useCallback(() => {
    const apply = (theme: string) => {
      document.documentElement.dataset.theme =
        theme === "light" ? "light" : "dark";
    };
    void api
      .getSetting("theme")
      .then((t) => apply(t ?? "dark"))
      // Defaulting to dark matches `:root`, which is what it did before.
      .catch(() => apply("dark"));
  }, []);

  // Refocus every time the window is shown — it stays alive between uses, so
  // without this the second invocation lands on a stale focus state. The theme
  // is re-read at the same moment, so switching theme while the app is open
  // takes effect on the next invocation rather than the next launch.
  useEffect(() => {
    const un = listen("capture:opened", () => {
      setText("");
      setParsed(null);
      setAttachments([]);
      syncTheme();
      requestAnimationFrame(() => inputRef.current?.focus());
    });
    syncTheme();
    inputRef.current?.focus();
    return () => {
      void un.then((f) => f());
    };
  }, [syncTheme]);

  // Debounced parse purely for the hint line. The save path does its own
  // parsing in Rust, so a slow or skipped preview can never affect what's saved.
  useEffect(() => {
    if (!text.trim()) {
      setParsed(null);
      return;
    }
    const t = setTimeout(() => {
      void api
        .saveCapturePreview(text)
        .then(setParsed)
        .catch(() => setParsed(null));
    }, 120);
    return () => clearTimeout(t);
  }, [text]);

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

  const hasHint = !!parsed && (!!parsed.subjectName || !!parsed.dueOn);

  return (
    // Spotlight's proportions: one tall pill, the input filling it, everything
    // else out of the way. The window is transparent and undecorated, so this
    // element *is* the visible object.
    //
    // `p-2` on the outer frame leaves room for the shadow to fall inside the
    // window bounds — an undecorated window clips anything drawn outside it.
    <div className="flex h-screen w-screen flex-col justify-center p-2">
      <div
        data-tauri-drag-region
        className="spotlight overflow-hidden rounded-[var(--r-xl)]"
        onPaste={onPaste}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            dismiss();
          }
          if (e.key === "Enter") {
            e.preventDefault();
            void save();
          }
        }}
      >
        {/* The whole bar drags except its controls — an undecorated window has
            no title bar, so without this it's stuck where macOS put it.
            `data-tauri-drag-region` rather than `-webkit-app-region`, which
            WKWebView ignores. Tauri only honours the attribute on the element
            actually under the pointer, so the controls opt out by being real
            interactive elements rather than by a second attribute. */}
        <div
          data-tauri-drag-region
          className="spotlight-drag flex items-center gap-3 px-4 py-3"
        >
          <Sparkle
            size={19}
            strokeWidth={1.9}
            className="shrink-0 text-[var(--ink-dim)]"
            aria-hidden
          />

          <input
            ref={inputRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Capture something…"
            spellCheck={false}
            autoComplete="off"
            aria-label="Quick capture"
            className="min-w-0 flex-1 bg-transparent text-[19px] leading-[1.4] tracking-[-0.01em] text-[var(--ink)] outline-none placeholder:text-[var(--ink-faint)]"
          />

          {/* Appears only when there's something to save, so an empty bar is
              completely quiet — the same way Spotlight shows nothing until you
              type. */}
          <button
            onClick={() => void attachFile()}
            title="Attach a screenshot or file"
            aria-label="Attach a screenshot or file"
            className="pressable shrink-0 rounded-[7px] p-1.5 text-[var(--ink-faint)] hover:bg-white/10 hover:text-[var(--ink)]"
          >
            <Paperclip size={15} />
          </button>

          <kbd
            className={cx(
              "flex h-[22px] shrink-0 items-center rounded-[7px] border px-1.5",
              "font-mono text-[11px] transition-opacity duration-[var(--t-base)]",
              text.trim() || attachments.length > 0
                ? "border-white/15 bg-white/10 text-[var(--ink-dim)] opacity-100"
                : "pointer-events-none border-transparent opacity-0",
            )}
          >
            <CornerDownLeft size={11} />
          </kbd>
        </div>

        {/* Attachments, shown so you can see what you actually grabbed. */}
        {attachments.length > 0 && (
          <div className="animate-rise flex flex-wrap gap-1.5 border-t border-white/10 px-4 py-2.5">
            {attachments.map((a, i) => (
              <span
                key={`${a.name}-${i}`}
                className="flex items-center gap-1.5 rounded-full border border-white/12 bg-white/8 px-2 py-1 text-[11.5px] text-[var(--ink-dim)]"
              >
                {a.imageDataUrl ? (
                  <img
                    src={a.imageDataUrl}
                    alt=""
                    className="h-4 w-4 rounded-[3px] object-cover"
                  />
                ) : (
                  <Paperclip size={10} />
                )}
                <span className="max-w-[140px] truncate">{a.name}</span>
                <button
                  onClick={() =>
                    setAttachments((list) => list.filter((_, j) => j !== i))
                  }
                  aria-label={`Remove ${a.name}`}
                  className="pressable text-[var(--ink-faint)] hover:text-[var(--ink)]"
                >
                  <X size={10} />
                </button>
              </span>
            ))}
          </div>
        )}

        {/* The result row exists only once the parser has something to say,
            so the bar is a single line at rest and grows into two. That's the
            Spotlight behaviour: results push the surface down, they don't sit
            in reserved empty space. */}
        {hasHint && (
          <div className="animate-rise border-t border-white/10 px-4 py-2.5">
            <div className="flex min-w-0 items-center gap-2 text-[12.5px]">
              {parsed!.subjectName && (
                <span className="shrink-0 rounded-full bg-[var(--accent)]/18 px-2 py-0.5 text-[var(--accent)]">
                  {parsed!.subjectName}
                </span>
              )}
              {parsed!.dueOn && (
                <span className="shrink-0 rounded-full bg-[var(--color-positive)]/18 px-2 py-0.5 text-[var(--color-positive)]">
                  due {parsed!.dueOn}
                </span>
              )}
              <span className="truncate text-[var(--ink-dim)]">
                {parsed!.title}
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

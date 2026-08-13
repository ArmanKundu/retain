import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CornerDownLeft, Sparkle } from "lucide-react";

import { cx } from "../components/ui";

import { api } from "../lib/api";
import type { ParsedCapture } from "../lib/types";

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
  const inputRef = useRef<HTMLInputElement>(null);

  // This window shares the bundle but not the app's store, so it never ran
  // `applyTheme` — which meant a light-mode app showed a dark capture bar.
  // Read the setting directly: one cheap SQLite lookup, no store, no bootstrap.
  // Set inline rather than via the store's `applyTheme`: importing that module
  // would pull zustand and the whole app store into this window, which is
  // exactly what `main.tsx` keeps out of the capture path.
  const syncTheme = useCallback(() => {
    const apply = (theme: string) => {
      document.documentElement.dataset.theme = theme === "light" ? "light" : "dark";
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
      void api.saveCapturePreview(text).then(setParsed).catch(() => setParsed(null));
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
    if (!value || saving) return;
    setSaving(true);
    try {
      await api.saveCapture(value);
      setText("");
      setParsed(null);
      await api.hideCaptureWindow();
    } finally {
      setSaving(false);
    }
  }, [text, saving]);

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
        className="spotlight overflow-hidden rounded-[var(--r-xl)]"
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
        <div className="flex items-center gap-3 px-4 py-3">
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
          <kbd
            className={cx(
              "flex h-[22px] shrink-0 items-center rounded-[7px] border px-1.5",
              "font-mono text-[11px] transition-opacity duration-[var(--t-base)]",
              text.trim()
                ? "border-white/15 bg-white/10 text-[var(--ink-dim)] opacity-100"
                : "pointer-events-none border-transparent opacity-0",
            )}
          >
            <CornerDownLeft size={11} />
          </kbd>
        </div>

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
              <span className="truncate text-[var(--ink-dim)]">{parsed!.title}</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { BarChart3, BookMarked, CalendarClock, ClipboardList, Dna, Home, Inbox as InboxIcon, Layers, MessageSquare, Settings as SettingsIcon, Timer as TimerIcon } from "lucide-react";

import { Onboarding } from "./screens/Onboarding";
import { Today } from "./screens/Today";
import { TimerScreen } from "./screens/TimerScreen";
import { Review } from "./screens/Review";
import { ImportScreen } from "./screens/Import";
import { ErrorLog } from "./screens/ErrorLog";
import { Inbox } from "./screens/Inbox";
import { Assessments } from "./screens/Assessments";
import { Biology } from "./screens/Biology";
import { Assistant } from "./screens/Assistant";
import { Library } from "./screens/Library";
import { Progress } from "./screens/Progress";
import { Settings } from "./screens/Settings";
import { FocusDock } from "./components/FocusDock";
import { SessionNotePrompt } from "./components/SessionNotePrompt";
import { cx } from "./components/ui";
import { isBiologyThreeFour } from "./lib/catalogue";
import { clock } from "./lib/format";
import type { FinishedSession, TimerSnapshot } from "./lib/types";
import { useApp, type Route } from "./store";

const NAV: { route: Route; label: string; Icon: typeof Home; onlyIf?: (s: AppSubjects) => boolean }[] = [
  { route: "today", label: "Today", Icon: Home },
  { route: "timer", label: "Timer", Icon: TimerIcon },
  { route: "inbox", label: "Inbox", Icon: InboxIcon },
  { route: "review", label: "Review", Icon: Layers },
  { route: "errors", label: "Error log", Icon: ClipboardList },
  { route: "assessments", label: "Assessments", Icon: CalendarClock },
  // Only shown when there is actually a Biology 3/4 subject — a nav item that
  // leads to an explanation of why it's empty is worse than no nav item.
  {
    route: "biology",
    label: "Biology 3/4",
    Icon: Dna,
    onlyIf: (subjects) => subjects.some(isBiologyThreeFour),
  },
  { route: "assistant", label: "Assistant", Icon: MessageSquare },
  { route: "library", label: "Library", Icon: BookMarked },
  { route: "progress", label: "Progress", Icon: BarChart3 },
  { route: "settings", label: "Settings", Icon: SettingsIcon },
];

type AppSubjects = ReturnType<typeof useApp.getState>["subjects"];

export default function App() {
  const { ready, boot, route, setRoute, timer, setTimer, init, refreshProgress, subjects } =
    useApp();
  const [justFinished, setJustFinished] = useState<FinishedSession | null>(null);

  useEffect(() => {
    void init();
  }, [init]);

  // The backend owns the timer and broadcasts it once a second. The UI is a
  // subscriber, which is why closing the window doesn't stop anything.
  useEffect(() => {
    const tick = listen<TimerSnapshot | null>("timer:tick", (e) => setTimer(e.payload));
    // Fired when a session is stopped from the menu bar, so the note prompt
    // still appears even though the stop didn't come from this window.
    const finished = listen<FinishedSession>("timer:finished", (e) => {
      setTimer(null);
      setJustFinished(e.payload);
    });

    return () => {
      void tick.then((un) => un());
      void finished.then((un) => un());
    };
  }, [setTimer]);

  if (!ready) {
    // The wash, not a flat panel — so the window never flashes grey on launch.
    return <div className="app-wash h-full" />;
  }

  if (!boot?.onboardingComplete) {
    return <Onboarding />;
  }

  return (
    <div className="app-wash flex h-full text-[var(--ink)]">
      {/* Sidebar.
          Deliberately quiet: no fill of its own, just a hairline against the
          app wash. A solid panel here competes with the content for attention,
          which is the opposite of what navigation should do. */}
      <nav className="flex w-[212px] shrink-0 flex-col border-r border-[var(--line-soft)]">
        {/* Padding for the traffic lights, which float over the content because
            the window uses an overlay title bar. */}
        <div className="titlebar-drag h-11 shrink-0" />

        <div className="flex flex-1 flex-col gap-0.5 px-2.5 py-2">
          {NAV.filter((n) => !n.onlyIf || n.onlyIf(subjects)).map(({ route: r, label, Icon }) => (
            <button
              key={r}
              onClick={() => {
                setRoute(r);
                if (r === "today" || r === "progress") void refreshProgress();
              }}
              aria-current={route === r ? "page" : undefined}
              className={cx(
                "pressable group relative flex items-center gap-2.5 rounded-[var(--r-md)]",
                "px-2.5 py-[7px] text-[13.5px] text-left",
                route === r
                  ? "font-medium text-[var(--ink)]"
                  : "text-[var(--ink-dim)] hover:bg-[var(--surface-hi)]/70 hover:text-[var(--ink)]",
              )}
              style={
                route === r
                  ? {
                      // A tinted pill rather than a saturated blue block: the
                      // active route should read as a selected object, not a
                      // web-app button.
                      background: "color-mix(in srgb, var(--accent) 13%, transparent)",
                      boxShadow: "inset 0 0 0 1px color-mix(in srgb, var(--accent) 20%, transparent)",
                    }
                  : undefined
              }
            >
              <Icon
                size={15.5}
                strokeWidth={1.9}
                className={cx(
                  "shrink-0 transition-colors duration-[var(--t-fast)]",
                  route === r ? "text-[var(--accent)]" : "",
                )}
              />
              <span className="truncate">{label}</span>
            </button>
          ))}
        </div>

        {timer && (
          <button
            onClick={() => setRoute("timer")}
            className="lift pressable mx-2.5 mb-2.5 rounded-[var(--r-md)] border border-[var(--line-soft)] bg-[var(--surface-hi)]/80 px-3 py-2.5 text-left"
          >
            <div className="flex items-center gap-2">
              <span
                className="h-1.5 w-1.5 shrink-0 rounded-full"
                style={{ background: timer.subjectColour }}
              />
              <span className="truncate text-[12px] text-[var(--ink-dim)]">
                {timer.subjectName}
              </span>
            </div>
            <div className="tabular mt-1 text-[17px] font-medium text-[var(--ink)]">
              {clock(timer.activeSeconds)}
              {timer.pausedReason && (
                <span className="ml-1.5 text-[11px] font-normal text-[var(--ink-faint)]">
                  {timer.pausedReason === "idle"
                    ? "idle"
                    : timer.pausedReason === "break"
                      ? "break"
                      : "paused"}
                </span>
              )}
            </div>
          </button>
        )}
      </nav>

      <main className="relative flex flex-1 flex-col overflow-y-auto">
        <div className="flex-1">
        {route === "today" && <Today />}
        {route === "timer" && <TimerScreen onFinished={setJustFinished} />}
        {route === "review" && <Review onImport={() => setRoute("import")} />}
        {route === "import" && <ImportScreen onDone={() => setRoute("review")} />}
        {route === "inbox" && <Inbox />}
        {route === "errors" && <ErrorLog />}
        {route === "assessments" && <Assessments />}
        {route === "biology" && <Biology />}
        {route === "library" && <Library />}
        {route === "assistant" && <Assistant />}
        {route === "progress" && <Progress />}
        {route === "settings" && <Settings />}
        </div>

        {/* A running session follows you across screens rather than being
            something you have to navigate back to. */}
        <FocusDock onFinished={setJustFinished} />
      </main>

      {justFinished && (
        <SessionNotePrompt
          session={justFinished}
          onClose={() => {
            setJustFinished(null);
            void refreshProgress();
          }}
        />
      )}
    </div>
  );
}

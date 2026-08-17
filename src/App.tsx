import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  BarChart3,
  BookMarked,
  HelpCircle,
  CalendarClock,
  CalendarDays,
  ClipboardList,
  Dna,
  Home,
  Inbox as InboxIcon,
  Layers,
  MessageSquare,
  NotebookPen,
  Settings as SettingsIcon,
  Timer as TimerIcon,
} from "lucide-react";

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
import { Week } from "./screens/Week";
import { Library } from "./screens/Library";
import { Notes } from "./screens/Notes";
import { Questions } from "./screens/Questions";
import { Progress } from "./screens/Progress";
import { Settings } from "./screens/Settings";
import { FocusDock } from "./components/FocusDock";
import { SessionNotePrompt } from "./components/SessionNotePrompt";
import { cx } from "./components/ui";
import { isBiologyThreeFour } from "./lib/catalogue";
import { clock } from "./lib/format";
import type { FinishedSession, TimerSnapshot } from "./lib/types";
import { api } from "./lib/api";
import { useApp, type Route } from "./store";

type NavItem = {
  route: Route;
  label: string;
  Icon: typeof Home;
  onlyIf?: (s: AppSubjects) => boolean;
};

/** Grouped so the list reads as three short columns rather than ten equal rows. */
const NAV_GROUPS: { heading: string; items: NavItem[] }[] = [
  {
    heading: "Study",
    items: [
      { route: "today", label: "Today", Icon: Home },
      { route: "timer", label: "Timer", Icon: TimerIcon },
      { route: "week", label: "Week", Icon: CalendarDays },
      { route: "inbox", label: "Inbox", Icon: InboxIcon },
      { route: "review", label: "Review", Icon: Layers },
    ],
  },
  {
    heading: "Learn",
    items: [
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
      { route: "notes", label: "Notes", Icon: NotebookPen },
      { route: "library", label: "Library", Icon: BookMarked },
      { route: "questions", label: "Past questions", Icon: HelpCircle },
    ],
  },
  {
    heading: "You",
    items: [
      { route: "progress", label: "Progress", Icon: BarChart3 },
      { route: "settings", label: "Settings", Icon: SettingsIcon },
    ],
  },
];

type AppSubjects = ReturnType<typeof useApp.getState>["subjects"];

export default function App() {
  const {
    ready,
    boot,
    route,
    setRoute,
    timer,
    setTimer,
    init,
    refreshProgress,
    subjects,
  } = useApp();
  const [justFinished, setJustFinished] = useState<FinishedSession | null>(
    null,
  );

  // Menu selections. Navigation is decided here rather than in Rust — the
  // router is React's, and a second copy of which screens exist would be one
  // more thing to keep in step with the sidebar.
  useEffect(() => {
    const un = listen<string>("menu", (event) => {
      const id = event.payload;
      if (id.startsWith("go:")) {
        setRoute(id.slice(3) as Route);
        return;
      }
      // ⌘P prints whatever is on screen, through the page stylesheet.
      if (id === "print") window.print();
      if (id === "new:note") setRoute("notes");
      if (id === "export") setRoute("settings");
      if (id === "help:updates") setRoute("settings");
      if (id === "timer:toggle")
        void api
          .pauseTimer()
          .then(setTimer)
          .catch(() => {});
      if (id === "timer:stop") setRoute("timer");
    });
    return () => {
      void un.then((f) => f());
    };
  }, [setRoute, setTimer]);

  useEffect(() => {
    void init();
  }, [init]);

  // The backend owns the timer and broadcasts it once a second. The UI is a
  // subscriber, which is why closing the window doesn't stop anything.
  useEffect(() => {
    const tick = listen<TimerSnapshot | null>("timer:tick", (e) =>
      setTimer(e.payload),
    );
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

        <div className="flex flex-1 flex-col gap-5 overflow-y-auto px-2.5 py-2">
          {NAV_GROUPS.map((group) => {
            const items = group.items.filter(
              (n) => !n.onlyIf || n.onlyIf(subjects),
            );
            if (items.length === 0) return null;

            return (
              <div key={group.heading}>
                <div className="px-2.5 pb-1.5 text-[11px] font-medium tracking-[0.04em] text-[var(--ink-faint)]">
                  {group.heading}
                </div>
                <div className="flex flex-col gap-0.5">
                  {items.map(({ route: r, label, Icon }) => (
                    <button
                      key={r}
                      onClick={() => {
                        setRoute(r);
                        if (r === "today" || r === "progress")
                          void refreshProgress();
                      }}
                      aria-current={route === r ? "page" : undefined}
                      className={cx(
                        "pressable group relative flex items-center gap-2.5 rounded-[var(--r-sm)]",
                        "px-2.5 py-[6px] text-left text-[13.5px]",
                        route === r
                          ? "font-medium text-[var(--ink)]"
                          : "text-[var(--ink-dim)] hover:bg-[var(--surface-hi)] hover:text-[var(--ink)]",
                      )}
                      style={
                        route === r
                          ? {
                              // A tinted capsule, not a saturated block: the
                              // active route should read as a selected object.
                              background:
                                "color-mix(in srgb, var(--accent) 11%, transparent)",
                            }
                          : undefined
                      }
                    >
                      <Icon
                        size={16}
                        strokeWidth={1.85}
                        className={cx(
                          "shrink-0 transition-colors duration-[var(--t-fast)]",
                          route === r ? "text-[var(--accent)]" : "",
                        )}
                      />
                      <span className="truncate">{label}</span>
                    </button>
                  ))}
                </div>
              </div>
            );
          })}
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
          {route === "week" && <Week />}
          {route === "review" && <Review onImport={() => setRoute("import")} />}
          {route === "import" && (
            <ImportScreen onDone={() => setRoute("review")} />
          )}
          {route === "inbox" && <Inbox />}
          {route === "errors" && <ErrorLog />}
          {route === "assessments" && <Assessments />}
          {route === "biology" && <Biology />}
          {route === "notes" && <Notes />}
          {route === "library" && <Library />}
          {route === "questions" && <Questions />}
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

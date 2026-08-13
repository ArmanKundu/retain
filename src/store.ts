// Application state.
//
// Note what this file does NOT do: it never calls Zustand's `persist` middleware.
// Everything durable lives in SQLite, and API keys live in the Keychain and are
// never sent to the frontend at all. There is deliberately no browser-storage
// write path anywhere in this app, which is the simplest way to guarantee a key
// can't leak into localStorage by someone later adding `persist` to a store that
// happens to hold one.

import { create } from "zustand";
import { api } from "./lib/api";
import { localDate, addDays } from "./lib/format";
import type {
  AiStatus,
  Bootstrap,
  GridDay,
  RecentSession,
  StreakSummary,
  Subject,
  TimerSnapshot,
  WeeklyGoalRing,
} from "./lib/types";

// `import` is deliberately not in the sidebar — it's reached from Review, which
// is where you notice you need more cards.
export type Route =
  | "today" | "timer" | "inbox" | "review" | "errors"
  | "assessments" | "biology" | "library" | "progress" | "settings" | "import";

interface AppState {
  ready: boolean;
  error: string | null;

  boot: Bootstrap | null;
  route: Route;

  subjects: Subject[];
  timer: TimerSnapshot | null;
  streak: StreakSummary | null;
  rings: WeeklyGoalRing[];
  grid: GridDay[];
  recent: RecentSession[];

  /**
   * Whether an AI provider key exists — never the key itself, which the
   * backend has no command to hand out.
   *
   * Held here rather than fetched per component because answering it reads the
   * Keychain once per provider, and the question gets asked from inside list
   * rows.
   */
  ai: AiStatus | null;
  refreshAi: () => Promise<void>;

  init: () => Promise<void>;
  setRoute: (route: Route) => void;
  setTimer: (t: TimerSnapshot | null) => void;
  refreshSubjects: () => Promise<void>;
  refreshProgress: () => Promise<void>;
  setTheme: (theme: "dark" | "light") => Promise<void>;
}

/** Shared in-flight `ai_status` request, so concurrent askers don't stack up. */
let inflightAi: Promise<void> | null = null;

/** A year of grid, ending today. */
function gridWindow(): [string, string] {
  const today = new Date();
  return [localDate(addDays(today, -364)), localDate(today)];
}

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  error: null,
  boot: null,
  route: "today",
  subjects: [],
  timer: null,
  streak: null,
  rings: [],
  grid: [],
  recent: [],
  ai: null,

  init: async () => {
    try {
      const boot = await api.bootstrap();
      applyTheme(boot.theme === "light" ? "light" : "dark");

      // A session may still be running from before the window was closed — the
      // backend keeps timing regardless of whether a UI exists.
      const timer = await api.getTimer();

      set({ boot, subjects: boot.subjects, timer, ready: true });

      if (boot.onboardingComplete) {
        await get().refreshProgress();
      }
    } catch (e) {
      set({ error: String(e), ready: true });
    }
  },

  refreshAi: async () => {
    // Several components can ask on the same render pass; they share one fetch
    // rather than each starting their own round of Keychain reads.
    inflightAi ??= (async () => {
      try {
        set({ ai: await api.aiStatus() });
      } catch {
        // Treated as "no key", never as an error — the AI parts simply aren't
        // offered and the rest of the app is unaffected.
        set({ ai: { provider: null, model: "", available: [] } });
      } finally {
        inflightAi = null;
      }
    })();

    return inflightAi;
  },

  setRoute: (route) => set({ route }),
  setTimer: (timer) => set({ timer }),

  refreshSubjects: async () => {
    set({ subjects: await api.listSubjects(false) });
  },

  refreshProgress: async () => {
    const [from, to] = gridWindow();
    const [streak, rings, grid, recent] = await Promise.all([
      api.streak(),
      api.weeklyRings(),
      api.grid(from, to),
      api.recentSessions(10),
    ]);
    set({ streak, rings, grid, recent });
  },

  setTheme: async (theme) => {
    applyTheme(theme);
    await api.setSetting("theme", theme);
    const boot = get().boot;
    if (boot) set({ boot: { ...boot, theme } });
  },
}));

/** The stylesheet keys off `data-theme` on the root element. */
export function applyTheme(theme: "dark" | "light") {
  document.documentElement.dataset.theme = theme;
}

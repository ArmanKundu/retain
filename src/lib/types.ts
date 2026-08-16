// Mirrors the structs in src-tauri/src/models.rs.
//
// These are hand-kept in sync rather than generated. With one backend, one
// frontend and four users, a codegen step would be more machinery than the
// problem needs — but if a field name here stops matching Rust, the value simply
// arrives as `undefined`, so treat this file as load-bearing.

export type UnitLevel = "1_2" | "3_4";
export type SubjectType = "science" | "maths" | "english" | "humanities";

export interface Subject {
  id: number;
  name: string;
  colour: string;
  unitLevel: UnitLevel;
  subjectType: SubjectType;
  weeklyGoalMinutes: number | null;
  sortOrder: number;
  archived: boolean;
}

export interface SubjectInput {
  name: string;
  colour: string;
  unitLevel: UnitLevel;
  subjectType: SubjectType;
  weeklyGoalMinutes: number | null;
}

export type TimerMode = "stopwatch" | "pomodoro";
export type Phase = "work" | "break";
export type PauseReason = "manual" | "idle" | "break";

export interface TimerSnapshot {
  sessionId: number;
  subjectId: number;
  subjectName: string;
  subjectColour: string;
  topicId: number | null;
  topicName: string | null;
  mode: TimerMode;
  phase: Phase;
  /** Wall clock since start. */
  elapsedSeconds: number;
  /** Elapsed minus pauses, idle and breaks. This is the number that counts. */
  activeSeconds: number;
  pauseCount: number;
  idlePauseCount: number;
  pausedReason: PauseReason | null;
  phaseRemainingSeconds: number | null;
  completedWorkBlocks: number;
}

export interface StartTimerInput {
  subjectId: number;
  topicId: number | null;
  mode: TimerMode;
  workMinutes: number | null;
  breakMinutes: number | null;
}

export interface FinishedSession {
  sessionId: number;
  subjectName: string;
  elapsedSeconds: number;
  activeSeconds: number;
  pauseCount: number;
  idlePauseCount: number;
  qualifiesForStreak: boolean;
}

export interface DaySubjectSlice {
  subjectId: number;
  subjectName: string;
  colour: string;
  minutes: number;
}

export interface GridDay {
  date: string;
  minutes: number;
  qualified: boolean;
  bySubject: DaySubjectSlice[];
}

export interface StreakSummary {
  current: number;
  longest: number;
  freezesAvailable: number;
  restDays: number[];
  todayQualified: boolean;
  todayActiveMinutes: number;
  thresholdMinutes: number;
}

export interface WeeklyGoalRing {
  subjectId: number;
  subjectName: string;
  colour: string;
  goalMinutes: number;
  doneMinutes: number;
}

export interface RecentSession {
  id: number;
  subjectName: string;
  colour: string;
  topicName: string | null;
  startedAt: string;
  activeSeconds: number;
  pauseCount: number;
  idlePauseCount: number;
  note: string | null;
}

export interface Bootstrap {
  onboardingComplete: boolean;
  userName: string;
  subjects: Subject[];
  focusedSessionMinutes: number;
  pomodoroWorkMinutes: number;
  pomodoroBreakMinutes: number;
  theme: string;
  appVersion: string;
}

// --- Flashcards -----------------------------------------------------------

export type Rating = "again" | "hard" | "good" | "easy";
export type CardState = "new" | "learning" | "review" | "relearning";
export type NoteType = "basic" | "cloze" | "quote";
export type Delimiter = "tab" | "semicolon" | "comma";

export interface QueueItem {
  cardId: number;
  subjectId: number;
  subjectName: string;
  colour: string;
  noteType: string;
  front: string;
  back: string;
  extra: string | null;
  clozeIndex: number | null;
  state: CardState;
  isNew: boolean;
}

export interface QueueCounts {
  /** Never capped — see the backend module docs. */
  dueReviews: number;
  newAvailable: number;
  newIntroducedToday: number;
  newPerDay: number;
  newRemainingTotal: number;
}

/**
 * What the FSRS backend decided. The UI never computes scheduling itself — it
 * sends a rating and renders whatever comes back.
 */
export interface AnswerResult {
  cardId: number;
  state: CardState;
  /** Days until the next review; null for an intraday learning step. */
  intervalDays: number | null;
  dueAt: string;
  stability: number;
  difficulty: number;
  reps: number;
  lapses: number;
}

export interface ParsedCard {
  noteType: NoteType;
  front: string;
  back: string;
  extra: string | null;
  clozeIndex: number | null;
  tags: string[];
}

export interface SkippedLine {
  lineNumber: number;
  text: string;
  reason: string;
}

export interface ImportPreview {
  delimiter: Delimiter;
  cards: ParsedCard[];
  /** Lines that produced no card, with why. Never silently dropped. */
  skipped: SkippedLine[];
}

export interface ImportResult {
  added: number;
  duplicates: number;
}

// --- Error log ------------------------------------------------------------

export interface ErrorEntryInput {
  subjectId: number;
  topicId: number | null;
  source: string | null;
  commandWord: string | null;
  questionText: string | null;
  questionImage: string | null;
  myAnswer: string | null;
  correctAnswer: string | null;
  category: string;
  fix: string | null;
  marksLost: number | null;
  marksAvailable: number | null;
}

export interface ErrorEntry {
  id: number;
  subjectId: number;
  subjectName: string;
  colour: string;
  topicId: number | null;
  topicName: string | null;
  loggedOn: string;
  source: string | null;
  commandWord: string | null;
  questionText: string | null;
  hasImage: boolean;
  myAnswer: string | null;
  correctAnswer: string | null;
  category: string;
  fix: string | null;
  marksLost: number | null;
  marksAvailable: number | null;
  revisitOn: string | null;
  fixedAt: string | null;
  reattemptCount: number;
}

export interface EntryFilter {
  subjectId?: number | null;
  category?: string | null;
  topicId?: number | null;
  search?: string | null;
  onlyUnfixed?: boolean | null;
}

/**
 * What you're shown during a blind re-attempt.
 *
 * There is deliberately no `correctAnswer` here — the backend type has no field
 * for it. The mark scheme arrives only from `revealErrorAnswer`, and only after
 * a blind answer has been committed.
 */
export interface BlindPrompt {
  reattemptId: number;
  entryId: number;
  subjectName: string;
  colour: string;
  topicName: string | null;
  source: string | null;
  commandWord: string | null;
  questionText: string | null;
  hasImage: boolean;
  marksAvailable: number | null;
  presentedAt: string;
}

export type SelfAssessment = "correct" | "partial" | "incorrect";

export interface CategoryCount {
  category: string;
  count: number;
  marksLost: number;
}

// --- Quick capture & inbox ------------------------------------------------

export interface ParsedCapture {
  title: string;
  subjectId: number | null;
  subjectName: string | null;
  dueOn: string | null;
  /** The exact words the parser consumed, so the UI can show its working. */
  matched: string[];
}

export interface Capture {
  id: number;
  rawText: string;
  createdAt: string;
  suggestedSubjectId: number | null;
  suggestedSubjectName: string | null;
  suggestedDueOn: string | null;
  suggestedTitle: string | null;
}

export interface Task {
  id: number;
  title: string;
  subjectId: number | null;
  subjectName: string | null;
  colour: string | null;
  dueOn: string | null;
  doneAt: string | null;
}

// --- Assessments & retrospective revision ---------------------------------

export type AssessmentKind = "sac" | "exam" | "other";

export interface AssessmentInput {
  subjectId: number;
  name: string;
  kind: AssessmentKind;
  dueOn: string;
  topicIds: number[] | null;
}

export interface Assessment {
  id: number;
  subjectId: number;
  subjectName: string;
  colour: string;
  name: string;
  kind: AssessmentKind;
  dueOn: string;
  daysAway: number;
  source: string;
  topicIds: number[];
  /** Dates to revise, counting backwards. Past ones are already dropped. */
  upcomingReviewPoints: string[];
}

export interface TopicStatus {
  topicId: number;
  topicName: string;
  subjectId: number;
  subjectName: string;
  colour: string;
  lastReviewedOn: string | null;
  daysSince: number | null;
  lastConfidence: number | null;
  reviewCount: number;
  priority: number;
}

export interface TopicRow {
  id: number;
  subjectId: number;
  name: string;
}

// --- Notifications --------------------------------------------------------

export type NotificationCategory =
  | "reviews"
  | "assessments"
  | "topic_decay"
  | "streak";

export interface NotificationSettings {
  enabled: boolean;
  /** Local hours. May wrap midnight (21 → 8 is the default). */
  quietFromHour: number;
  quietToHour: number;
  dailyCap: number;
  reviews: boolean;
  assessments: boolean;
  topicDecay: boolean;
  streak: boolean;
}

export interface NotificationCandidate {
  category: NotificationCategory;
  title: string;
  body: string;
  dedupeKey: string;
  cooldownDays: number;
}

export type Provider = "anthropic" | "open_ai" | "gemini" | "open_router";

/**
 * The result of checking a key with its provider.
 *
 * Three outcomes, not two. `unreachable` is deliberately distinct from
 * `invalid`: the first means we never got an answer, the second means we got a
 * "no". Treating them the same would reject a good key whenever the network is
 * down, and the app is supposed to work offline.
 */
export type KeyCheck =
  | { status: "valid"; detail: string | null }
  | { status: "invalid"; message: string }
  | { status: "unreachable"; message: string };

export interface ImportReport {
  tablesWritten: number;
  rowsWritten: number;
}

// --- AI ---------------------------------------------------------------------
//
// Every AI type is a *suggestion* type. None of them is ever written straight to
// the database — each one lands in an editable form the user confirms first.

export interface AiStatus {
  /** null means no key anywhere, and the UI shows the "add a key" state. */
  provider: Provider | null;
  model: string;
  available: Provider[];
}

export interface TaskSuggestion {
  title: string;
  subject: string | null;
  dueOn: string | null;
}

export interface CardSuggestion {
  front: string;
  back: string;
}

/**
 * The week's numbers, computed in Rust from the database — never generated.
 * This is available with or without an API key.
 */
export interface WeeklyFacts {
  minutesBySubject: [string, number][];
  untouched: string[];
  errorsByCategory: [string, number][];
  totalMinutes: number;
  sessions: number;
  cardsReviewed: number;
  from: string;
  to: string;
}

export interface WeeklyReview {
  facts: WeeklyFacts;
  /** null when the week was too quiet to be worth writing about. */
  prose: string | null;
}

// --- Calendar (ICS subscription) --------------------------------------------

export interface CalendarEvent {
  uid: string;
  recurrenceId: string | null;
  summary: string;
  description: string | null;
  /** RFC 3339, UTC. */
  startsAt: string;
  endsAt: string | null;
  allDay: boolean;
  /** The local calendar date the event belongs to. */
  localDate: string;
}

export interface CalendarStatus {
  enabled: boolean;
  url: string;
  lastSyncAt: string | null;
  /** Empty string means no error. Kept separate from a failed promise: an
   *  unreachable calendar is an expected state, not an exception. */
  lastError: string | null;
  eventCount: number;
}

/** One entry in the command-word reference. */
export interface CommandWord {
  word: string;
  meaning: string;
}

// --- Biology 3/4 -------------------------------------------------------------
//
// No VCAA content ships in the app. The topic tree is a structure you fill from
// your own copy of the study design via the outline importer.

export interface TopicNode {
  id: number;
  name: string;
  /** "unit" | "aos" | "dot_point", or null for free-form. */
  kind: string | null;
  children: TopicNode[];
  /** Progress for this node only — a parent does not inherit a child's. */
  confidence: number | null;
  lastReviewedOn: string | null;
  cardCount: number;
  errorCount: number;
}

export interface OutlineRow {
  depth: number;
  name: string;
  kind: string;
}

export type ExamPhase = "reading" | "writing" | "finished";

export interface ExamRun {
  subjectId: number;
  name: string;
  startedAt: string;
  pausedAt: string | null;
  pausedSeconds: number;
}

export interface ExamState {
  run: ExamRun;
  phase: ExamPhase;
  /** Excludes paused time. */
  elapsedSeconds: number;
  remainingSeconds: number;
  paused: boolean;
  totalSeconds: number;
}

export interface PracticeExam {
  id: number;
  name: string;
  takenOn: string;
  sectionAScore: number | null;
  sectionAMax: number;
  sectionBScore: number | null;
  sectionBMax: number;
  readingSeconds: number | null;
  writingSeconds: number | null;
}

export interface DeckSummary {
  total: number;
  due: number;
  new: number;
}

// --- Update check ------------------------------------------------------------
//
// Reports only. Retain never downloads or installs an update — it's ad-hoc
// signed, so replacing its own bundle would be indistinguishable from something
// malicious doing the same.

export type UpdateStatus =
  | { status: "upToDate"; current: string }
  | {
      status: "available";
      current: string;
      latest: string;
      url: string;
      notes: string | null;
      /** Direct link to the .dmg, when the release has one. Null means the
          update can be opened in a browser but not installed for you. */
      downloadUrl: string | null;
    }
  /** Distinct from upToDate on purpose: we never got an answer. */
  | { status: "unknown"; current: string; reason: string };

export interface UpdateReport {
  status: UpdateStatus;
  checkedAt: string | null;
  releasesPage: string;
}

/** A model the configured key can see. */
export interface ModelOption {
  id: string;
  displayName: string;
  /** Retain only performs generateContent; anything else is unusable here. */
  supportsGenerateContent: boolean;
}

/** What a rating would schedule, previewed before you answer. */
export interface IntervalPreview {
  rating: Rating;
  /** Whole days, or null for an intraday learning step. */
  intervalDays: number | null;
}

// --- Your material, and what the AI made from it -----------------------------

export type ResourceKind =
  | "study_design"
  | "past_paper"
  /** A school's practice exam — a prediction of a VCAA paper, not one. */
  | "trial_test"
  | "exam_solution"
  | "school_notes"
  | "personal_notes"
  | "textbook"
  | "other";

export interface Resource {
  id: number;
  subjectId: number | null;
  subjectName: string | null;
  title: string;
  kind: ResourceKind;
  /**
   * 3, 4, or null for material that spans the sequence.
   *
   * Null is a real answer, not missing data: a study design covers both units
   * and a VCAA exam examines both in one paper. Only notes and trial tests are
   * genuinely filed per unit.
   */
  unit: number | null;
  source: string | null;
  wordCount: number;
  chunkCount: number;
  addedAt: string;
}

/** A retrieved passage of your own material, with enough provenance to check it. */
export interface Excerpt {
  resourceId: number;
  resourceTitle: string;
  kind: ResourceKind;
  ordinal: number;
  content: string;
}

/** AI text plus the excerpts of your material that shaped it. */
export interface GroundedText {
  body: string;
  sources: Excerpt[];
}

export type LibraryKind =
  | "notes"
  | "practice_question"
  | "weekly_review"
  | "answer"
  | "cards";

export interface LibraryItem {
  id: number;
  subjectId: number | null;
  subjectName: string | null;
  colour: string | null;
  kind: LibraryKind;
  title: string;
  /** What was asked, kept so an item is reproducible. */
  prompt: string | null;
  body: string;
  model: string | null;
  pinned: boolean;
  createdAt: string;
}

export interface LibraryFilter {
  subjectId?: number | null;
  kind?: string | null;
  search?: string | null;
  onlyPinned?: boolean | null;
}

// --- Folders, file import, and the assistant ---------------------------------

export interface SubjectFolder {
  subjectId: number;
  subjectName: string;
  colour: string;
  path: string;
  fileCount: number;
  importedCount: number;
}

/** What happened to one file during import. */
export type Outcome =
  | {
      status: "extracted";
      path: string;
      name: string;
      text: string;
      words: number;
    }
  /** A PDF whose pages are images — there is no text to extract. */
  | { status: "scanned"; path: string; name: string }
  | { status: "unsupported"; path: string; name: string; reason: string }
  | { status: "failed"; path: string; name: string; reason: string };

export interface ImportedFile {
  name: string;
  outcome: Outcome;
  resourceId: number | null;
  skippedDuplicate: boolean;
}

/**
 * How the assistant may answer.
 *
 * `strict` — only from your material; gaps are stated, not filled.
 * `open`   — your material first, then general knowledge, labelled as such.
 */
export type Grounding = "strict" | "open";

export interface Conversation {
  id: number;
  subjectId: number | null;
  subjectName: string | null;
  colour: string | null;
  title: string;
  grounding: Grounding;
  messageCount: number;
  updatedAt: string;
}

export interface MessageAttachment {
  id: number;
  name: string;
  words: number;
}

export interface ChatMessage {
  id: number;
  role: "user" | "assistant";
  body: string;
  /** Citations — which passages of your material grounded this answer. */
  sources: Excerpt[];
  model: string | null;
  attachments: MessageAttachment[];
  createdAt: string;
}

export interface NewAttachment {
  name: string;
  /** Extracted text. Empty for an image, whose content is the image itself. */
  content: string;
  /** A `data:image/...;base64,` URL — a screenshot or a photo. */
  imageDataUrl: string | null;
}

/**
 * Something the assistant offered to do, which you have not agreed to yet.
 *
 * `summary` is generated in Rust from the parsed action, never written by the
 * model — see `tools.rs`. A button whose label came from the same place as the
 * action it performs is not a confirmation.
 */
export interface Proposal {
  action: Record<string, unknown> & { action: string };
  summary: string;
  /** True for anything that leaves Retain. Shown more prominently. */
  external: boolean;
}

export interface AssistantTurn {
  message: ChatMessage;
  proposals: Proposal[];
}

export interface Applied {
  ok: boolean;
  message: string;
  open: string | null;
}

/** How one day was actually spent — the question the grid always prompted. */
export interface DaySubject {
  subjectId: number;
  subjectName: string;
  colour: string;
  minutes: number;
  sessions: number;
}

export interface DayDetail {
  localDate: string;
  totalMinutes: number;
  sessionCount: number;
  qualified: boolean;
  bySubject: DaySubject[];
  notes: string[];
}

// --- Time blocks: when you can't study ---------------------------------------

export type BlockKind =
  | "class"
  | "tuition"
  | "work"
  | "commute"
  | "exercise"
  | "family"
  | "rest"
  | "other";

export interface TimeBlock {
  id: number;
  title: string;
  kind: BlockKind;
  /** 0 = Monday. Set for a weekly commitment. */
  weekday: number | null;
  /** Set for a one-off. */
  onDate: string | null;
  /** Minutes from local midnight. */
  startMin: number;
  endMin: number;
  /** Whether study can happen here — a class you can revise through, say. */
  available: boolean;
  subjectId: number | null;
  subjectName: string | null;
  colour: string | null;
  note: string | null;
  /** A meeting URL, opened in the browser from the week grid. */
  link: string | null;
}

export interface NewBlock {
  title: string;
  kind: BlockKind;
  weekday: number | null;
  onDate: string | null;
  startMin: number;
  endMin: number;
  available: boolean;
  subjectId: number | null;
  note: string | null;
  link: string | null;
}

/** Something attached to a quick capture — a screenshot, or a file's text. */
export interface NewCaptureAttachment {
  name: string;
  /** `data:image/...;base64,...` for a pasted or dropped image. */
  imageDataUrl: string | null;
  text: string | null;
}

export interface CaptureAttachment {
  id: number;
  name: string;
  kind: "image" | "text";
  imageDataUrl: string | null;
  text: string | null;
}

/**
 * One intention: a thing you meant to do, on a day.
 *
 * `firstPlannedOn` and `moves` exist so the UI can say "you've been meaning to
 * do this since Tuesday" — a plan that quietly rewrites its own history can't
 * tell you the one thing worth knowing.
 */
export interface PlanItem {
  id: number;
  subjectId: number | null;
  subjectName: string | null;
  colour: string | null;
  title: string;
  detail: string | null;
  plannedOn: string;
  firstPlannedOn: string;
  estMinutes: number;
  dueOn: string | null;
  status: "planned" | "done" | "skipped";
  moves: number;
  source: "manual" | "ai" | "assessment";
}

export interface NewPlanItem {
  subjectId: number | null;
  title: string;
  detail: string | null;
  plannedOn: string;
  estMinutes: number;
  dueOn: string | null;
  source: string | null;
}

/** What rollover did, so the change can be shown rather than just applied. */
export interface Rollover {
  moved: {
    id: number;
    title: string;
    subjectName: string | null;
    from: string;
    to: string;
    moves: number;
  }[];
  stuck: {
    id: number;
    title: string;
    subjectName: string | null;
    from: string;
    reason: string;
  }[];
}

/**
 * One entry on a day's timetable, decoded from what Compass sends.
 *
 * Compass splits a class across three ICS properties and none is a sentence:
 * SUMMARY is a code (`11CHEU2`), LOCATION is a room, DESCRIPTION is the
 * teacher. `subjectName` is null when the code isn't one of your subjects — an
 * assembly, a formal — and inventing one for those is worse than silence.
 */
export interface ScheduledClass {
  code: string;
  subjectName: string | null;
  colour: string | null;
  room: string | null;
  teacher: string | null;
  startsAt: string;
  endsAt: string | null;
  allDay: boolean;
}

/**
 * How much of a deck you actually hold.
 *
 * `mastered` counts cards whose FSRS stability is a fortnight or more — not
 * cards you got right recently. A card answered correctly twice today scores
 * 100% on accuracy and will be gone by Thursday; stability is about the memory
 * rather than the last quiz. See `src-tauri/src/mastery.rs`.
 */
export interface Strength {
  total: number;
  new: number;
  learning: number;
  mastered: number;
  /** Forgotten eight times or more — worth rewriting, not redoing. */
  leeches: number;
  suspended: number;
  dueToday: number;
  /** 0 when the deck is empty, never 1. */
  mastery: number;
  nextDueOn: string | null;
}

export interface SubjectMastery extends Strength {
  subjectId: number;
  name: string;
  colour: string;
}

export interface TopicMastery extends Strength {
  /** Null is the real bucket for unfiled cards, which is most of them. */
  topicId: number | null;
  name: string;
}

export interface DayAccuracy {
  date: string;
  reviews: number;
  accuracy: number;
}

export interface DeckStats extends Strength {
  recentReviews: number;
  /** Null when nothing has been reviewed — different from 0%. */
  recentAccuracy: number | null;
  recent: DayAccuracy[];
  averageStability: number | null;
}

/** Days until the card is next due, per rating. Null = back today. */
export interface RatingPreview {
  again: number | null;
  hard: number | null;
  good: number | null;
  easy: number | null;
}

// Typed wrappers around Tauri's `invoke`.
//
// Every call into Rust goes through here rather than components calling `invoke`
// with a string directly. That way the command names exist in exactly one place,
// and a rename is a compile error instead of a runtime one.

import { invoke } from "@tauri-apps/api/core";
import type {
  AiStatus,
  AnswerResult,
  Assessment,
  AssessmentInput,
  BlindPrompt,
  Bootstrap,
  CategoryCount,
  Delimiter,
  EntryFilter,
  ErrorEntry,
  ErrorEntryInput,
  SelfAssessment,
  FinishedSession,
  GridDay,
  ImportPreview,
  IntervalPreview,
  ImportReport,
  ImportResult,
  CalendarEvent,
  CalendarStatus,
  CommandWord,
  DayDetail,
  DeckSummary,
  ChatMessage,
  Conversation,
  Excerpt,
  ExamState,
  GroundedText,
  Grounding,
  ImportedFile,
  LibraryFilter,
  LibraryItem,
  ModelOption,
  NewAttachment,
  Outcome,
  Resource,
  ResourceKind,
  OutlineRow,
  PracticeExam,
  SubjectFolder,
  TopicNode,
  UpdateReport,
  CardSuggestion,
  Capture,
  KeyCheck,
  NotificationCandidate,
  NotificationSettings,
  ParsedCapture,
  TaskSuggestion,
  WeeklyFacts,
  WeeklyReview,
  Task,
  TopicRow,
  TopicStatus,
  Provider,
  QueueCounts,
  QueueItem,
  Rating,
  RecentSession,
  StartTimerInput,
  StreakSummary,
  Subject,
  SubjectInput,
  TimerSnapshot,
  WeeklyGoalRing,
} from "./types";

export const api = {
  // --- bootstrap & settings ---
  bootstrap: () => invoke<Bootstrap>("get_bootstrap"),
  completeOnboarding: (name: string) => invoke<void>("complete_onboarding", { name }),
  getSetting: (key: string) => invoke<string | null>("get_setting", { key }),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  setRestDays: (weekdays: number[]) => invoke<void>("set_rest_days", { weekdays }),

  // --- subjects ---
  listSubjects: (includeArchived = false) =>
    invoke<Subject[]>("list_subjects", { includeArchived }),
  createSubject: (input: SubjectInput) => invoke<Subject>("create_subject", { input }),
  updateSubject: (id: number, input: SubjectInput) =>
    invoke<Subject>("update_subject", { id, input }),
  archiveSubject: (id: number) => invoke<void>("archive_subject", { id }),
  unarchiveSubject: (id: number) => invoke<void>("unarchive_subject", { id }),
  reorderSubjects: (orderedIds: number[]) => invoke<void>("reorder_subjects", { orderedIds }),
  setWeeklyGoal: (id: number, minutes: number | null) =>
    invoke<void>("set_weekly_goal", { id, minutes }),

  // --- timer ---
  startTimer: (input: StartTimerInput) => invoke<TimerSnapshot>("start_timer", { input }),
  pauseTimer: () => invoke<TimerSnapshot | null>("pause_timer"),
  resumeTimer: () => invoke<TimerSnapshot | null>("resume_timer"),
  stopTimer: () => invoke<FinishedSession | null>("stop_timer"),
  getTimer: () => invoke<TimerSnapshot | null>("get_timer"),
  /** Delete a just-finished session. Only works once it has ended. */
  discardSession: (sessionId: number) => invoke<void>("discard_session", { sessionId }),
  setSessionNote: (sessionId: number, note: string | null) =>
    invoke<void>("set_session_note", { sessionId, note }),
  /** Per-subject breakdown for one day. */
  dayDetail: (localDate: string) => invoke<DayDetail>("day_detail", { localDate }),
  recentSessions: (limit = 12) => invoke<RecentSession[]>("recent_sessions", { limit }),

  // --- grid, streak, goals ---
  grid: (from: string, to: string) => invoke<GridDay[]>("get_grid", { from, to }),
  streak: () => invoke<StreakSummary>("get_streak"),
  weeklyRings: () => invoke<WeeklyGoalRing[]>("get_weekly_rings"),

  // --- API keys ---
  //
  // Note what is missing: there is no `getSecret`. The backend deliberately
  // exposes no command that returns a key, so the frontend cannot read one even
  // by mistake. It can only ask whether one exists.
  // --- flashcards ---
  //
  // Note the asymmetry: the UI sends a *rating* and receives the schedule. It
  // never computes an interval itself — FSRS lives entirely in Rust.
  reviewQueue: (subjectId: number | null, limit = 200) =>
    invoke<QueueItem[]>("review_queue", { subjectId, limit }),
  reviewCounts: (subjectId: number | null = null) =>
    invoke<QueueCounts>("review_counts", { subjectId }),
  /** What each rating would schedule. Read-only; records nothing. */
  previewIntervals: (cardId: number) =>
    invoke<IntervalPreview[]>("preview_intervals", { cardId }),
  answerCard: (cardId: number, rating: Rating, presentedAt: string) =>
    invoke<AnswerResult>("answer_card", { cardId, rating, presentedAt }),
  reviewForecast: (days = 30) => invoke<[string, number][]>("review_forecast", { days }),
  previewCardImport: (text: string, delimiter: Delimiter | null, quoteMode: boolean) =>
    invoke<ImportPreview>("preview_card_import", { text, delimiter, quoteMode }),
  importCards: (
    subjectId: number,
    topicId: number | null,
    text: string,
    delimiter: Delimiter | null,
    quoteMode: boolean,
  ) =>
    invoke<ImportResult>("import_cards", {
      subjectId,
      topicId,
      text,
      delimiter,
      quoteMode,
    }),

  // --- quick capture & inbox ---
  //
  // `saveCapturePreview` parses without writing; only `saveCapture` stores.
  saveCapturePreview: (text: string) => invoke<ParsedCapture>("preview_capture", { text }),
  saveCapture: (text: string) => invoke<ParsedCapture>("save_capture", { text }),
  hideCaptureWindow: () => invoke<void>("hide_capture_window"),
  listInbox: () => invoke<Capture[]>("list_inbox"),
  inboxCount: () => invoke<number>("inbox_count"),
  triageCaptureToTask: (
    captureId: number,
    title: string,
    subjectId: number | null,
    dueOn: string | null,
  ) => invoke<number>("triage_capture_to_task", { captureId, title, subjectId, dueOn }),
  triageCapture: (captureId: number, destination: "card" | "error_entry" | "discarded") =>
    invoke<void>("triage_capture", { captureId, destination }),
  listTasks: (includeDone = false) => invoke<Task[]>("list_tasks", { includeDone }),
  setTaskDone: (id: number, done: boolean) => invoke<void>("set_task_done", { id, done }),
  deleteTask: (id: number) => invoke<void>("delete_task", { id }),

  // --- assessments & retrospective revision ---
  //
  // `surfaceTopics` is recomputed on every call and stored nowhere. That is what
  // makes it retrospective: there is no plan to fall behind on.
  createAssessment: (input: AssessmentInput) => invoke<number>("create_assessment", { input }),
  listAssessments: (includePast = false) =>
    invoke<Assessment[]>("list_assessments", { includePast }),
  deleteAssessment: (id: number) => invoke<void>("delete_assessment", { id }),
  logTopicReview: (topicId: number, confidence: number, note: string | null = null) =>
    invoke<void>("log_topic_review", { topicId, confidence, note }),
  surfaceTopics: (subjectId: number | null = null, limit = 25) =>
    invoke<TopicStatus[]>("surface_topics", { subjectId, limit }),
  createTopic: (subjectId: number, name: string) =>
    invoke<number>("create_topic", { subjectId, name }),
  listTopics: (subjectId: number | null = null) =>
    invoke<TopicRow[]>("list_topics", { subjectId }),
  deleteTopic: (id: number) => invoke<void>("delete_topic", { id }),

  // --- notifications ---
  //
  // `previewNotifications` evaluates the rules WITHOUT sending or recording, so
  // Settings can show exactly what you'd receive before you turn a category on.
  notificationSettings: () => invoke<NotificationSettings>("notification_settings"),
  setNotificationSettings: (settings: NotificationSettings) =>
    invoke<void>("set_notification_settings", { settings }),
  notificationsSentToday: () => invoke<number>("notifications_sent_today"),
  previewNotifications: () => invoke<NotificationCandidate[]>("preview_notifications"),

  // --- error log ---
  /** Subject-aware: Biology 3/4 gets its course categories on top of Science. */
  errorCategories: (subjectId: number) =>
    invoke<string[]>("error_categories", { subjectId }),
  commandWords: () => invoke<CommandWord[]>("command_words"),
  createErrorEntry: (input: ErrorEntryInput) => invoke<number>("create_error_entry", { input }),
  listErrorEntries: (filter: EntryFilter) => invoke<ErrorEntry[]>("list_error_entries", { filter }),
  deleteErrorEntry: (id: number) => invoke<void>("delete_error_entry", { id }),
  errorEntryImage: (id: number) => invoke<string | null>("error_entry_image", { id }),
  dueErrorReattempts: (subjectId: number | null = null) =>
    invoke<number[]>("due_error_reattempts", { subjectId }),

  // The blind re-attempt sequence. `startErrorReattempt` returns a prompt with
  // no answer in it; `revealErrorAnswer` is the only way to obtain the mark
  // scheme and rejects the call if nothing has been committed.
  startErrorReattempt: (entryId: number) =>
    invoke<BlindPrompt>("start_error_reattempt", { entryId }),
  commitErrorReattempt: (reattemptId: number, blindAnswer: string) =>
    invoke<void>("commit_error_reattempt", { reattemptId, blindAnswer }),
  revealErrorAnswer: (reattemptId: number) =>
    invoke<string | null>("reveal_error_answer", { reattemptId }),
  assessErrorReattempt: (
    reattemptId: number,
    assessment: SelfAssessment,
    marksAwarded: number | null,
  ) => invoke<boolean>("assess_error_reattempt", { reattemptId, assessment, marksAwarded }),

  recurringErrors: (subjectId: number | null, days = 30) =>
    invoke<CategoryCount[]>("recurring_errors", { subjectId, days }),

  /** Check the key with its provider, and store it only if the provider says yes. */
  secretVerifyAndStore: (provider: Provider, key: string) =>
    invoke<KeyCheck>("secret_verify_and_store", { provider, key }),
  /** Offline escape hatch — only offered after a check came back `unreachable`. */
  secretStoreUnverified: (provider: Provider, key: string) =>
    invoke<void>("secret_store_unverified", { provider, key }),
  /** Re-check an already-stored key. The key itself never comes back to the UI. */
  secretTestStored: (provider: Provider) => invoke<KeyCheck>("secret_test_stored", { provider }),

  secretSet: (provider: Provider, key: string) => invoke<void>("secret_set", { provider, key }),
  secretHas: (provider: Provider) => invoke<boolean>("secret_has", { provider }),
  secretDelete: (provider: Provider) => invoke<void>("secret_delete", { provider }),

  // --- AI (optional; every one of these needs a key) ---
  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiSetProvider: (provider: Provider) => invoke<void>("ai_set_provider", { provider }),
  aiSetModel: (provider: Provider, model: string) =>
    invoke<void>("ai_set_model", { provider, model }),

  /** Returns a suggestion to edit and confirm — writes nothing. */
  aiTaskFromNote: (note: string) => invoke<TaskSuggestion>("ai_task_from_note", { note }),
  /** Returns suggestions; nothing reaches the deck until the user accepts. */
  aiCardsFromNotes: (subjectId: number, notes: string, count: number) =>
    invoke<CardSuggestion[]>("ai_cards_from_notes", { subjectId, notes, count }),
  aiWeeklyReview: () => invoke<WeeklyReview>("ai_weekly_review"),
  /** Models the key can see. Empty for providers that publish no list. */
  listAiModels: (provider: Provider) => invoke<ModelOption[]>("list_ai_models", { provider }),
  /** Performs a real minimal generation — the only trustworthy check. */
  testAiModel: (provider?: Provider, model?: string) =>
    invoke<string>("test_ai_model", { provider: provider ?? null, model: model ?? null }),
  /** The numbers alone — no key, no network. */
  weeklyFacts: () => invoke<WeeklyFacts>("weekly_facts"),
  /** Grounded in your own material when it covers the topic. Auto-archived. */
  aiPracticeQuestion: (dotPoint: string, marks: number, subjectId: number | null = null) =>
    invoke<GroundedText>("ai_practice_question", { dotPoint, marks, subjectId }),
  /** Study notes on a topic, grounded in your material. Auto-archived. */
  aiNotes: (topic: string, subjectId: number | null = null) =>
    invoke<GroundedText>("ai_notes", { topic, subjectId }),
  /** Suggestion only; null means the model didn't pick an allowed category. */
  aiSuggestCategory: (
    subjectId: number,
    question: string,
    myAnswer: string,
    correctAnswer: string,
  ) => invoke<string | null>("ai_suggest_category", { subjectId, question, myAnswer, correctAnswer }),

  // --- calendar (ICS subscription only; no login, no scraping) ---
  calendarStatus: () => invoke<CalendarStatus>("calendar_status"),
  setCalendarSettings: (enabled: boolean, url: string) =>
    invoke<CalendarStatus>("set_calendar_settings", { enabled, url }),
  /** Resolves even when the fetch failed — read `lastError` on the result. */
  syncCalendar: () => invoke<CalendarStatus>("sync_calendar"),
  upcomingEvents: (days: number, limit: number) =>
    invoke<CalendarEvent[]>("upcoming_events", { days, limit }),
  clearCalendar: () => invoke<CalendarStatus>("clear_calendar"),

  // --- biology 3/4 ---
  topicTree: (subjectId: number) => invoke<TopicNode[]>("topic_tree", { subjectId }),
  /** Parse an outline without writing anything. */
  previewTopicOutline: (text: string) => invoke<OutlineRow[]>("preview_topic_outline", { text }),
  /** Destructive: replaces the subject's topics. Cards/errors keep existing but
   *  lose their topic link. */
  importTopicOutline: (subjectId: number, text: string) =>
    invoke<number>("import_topic_outline", { subjectId, text }),
  terminologySummary: (subjectId: number) =>
    invoke<DeckSummary>("terminology_summary", { subjectId }),

  // exam simulation — state is derived from a stored start instant, so quitting
  // mid-exam and reopening resumes rather than restarts.
  examState: () => invoke<ExamState | null>("exam_state"),
  startExam: (subjectId: number, name: string) =>
    invoke<ExamState>("start_exam", { subjectId, name }),
  setExamPaused: (paused: boolean) => invoke<ExamState>("set_exam_paused", { paused }),
  finishExam: () => invoke<number>("finish_exam"),
  cancelExam: () => invoke<void>("cancel_exam"),
  scoreExam: (examId: number, sectionA: number | null, sectionB: number | null) =>
    invoke<void>("score_exam", { examId, sectionA, sectionB }),
  examHistory: (subjectId: number, limit = 10) =>
    invoke<PracticeExam[]>("exam_history", { subjectId, limit }),

  // --- update check (reports only; never downloads or installs) ---
  /** Cached result. No network, works offline. */
  updateStatus: () => invoke<UpdateReport>("update_status"),
  /** Asks GitHub now. Resolves even on failure — read `status`. */
  checkForUpdates: () => invoke<UpdateReport>("check_for_updates"),
  /** Opens a github.com release page in the default browser. */
  openReleasePage: (url?: string) => invoke<void>("open_release_page", { url: url ?? null }),

  // --- your material ---
  listResources: (subjectId: number | null = null) =>
    invoke<Resource[]>("list_resources", { subjectId }),
  addResource: (
    subjectId: number | null,
    title: string,
    kind: ResourceKind,
    source: string | null,
    content: string,
  ) => invoke<number>("add_resource", { subjectId, title, kind, source, content }),
  deleteResource: (id: number) => invoke<void>("delete_resource", { id }),
  /** What would be retrieved for a question. No model call, so it's free. */
  searchResources: (question: string, subjectId: number | null = null, limit = 6) =>
    invoke<Excerpt[]>("search_resources", { question, subjectId, limit }),

  // --- library of saved AI output ---
  listLibrary: (filter: LibraryFilter = {}, limit = 200) =>
    invoke<LibraryItem[]>("list_library", { filter, limit }),
  setLibraryPinned: (id: number, pinned: boolean) =>
    invoke<void>("set_library_pinned", { id, pinned }),
  renameLibraryItem: (id: number, title: string) =>
    invoke<void>("rename_library_item", { id, title }),
  deleteLibraryItem: (id: number) => invoke<void>("delete_library_item", { id }),
  libraryItemMarkdown: (id: number) => invoke<string>("library_item_markdown", { id }),
  /** Writes a .md into Downloads and returns the path. */
  exportLibraryItem: (id: number) => invoke<string>("export_library_item", { id }),

  // --- subject folders and file import ---
  /** Creates ~/Documents/Retain/<Subject>/ for each subject. Idempotent. */
  ensureSubjectFolders: () => invoke<SubjectFolder[]>("ensure_subject_folders"),
  revealFolder: (path: string) => invoke<void>("reveal_folder", { path }),
  /** Reads every readable file in a folder. Already-imported files are skipped. */
  importFolder: (path: string, subjectId: number | null) =>
    invoke<ImportedFile[]>("import_folder", { path, subjectId }),
  /** Extracts text from one file — used for chat attachments. */
  readFileText: (path: string) => invoke<Outcome>("read_file_text", { path }),

  // --- the assistant ---
  listConversations: (limit = 100) => invoke<Conversation[]>("list_conversations", { limit }),
  createConversation: (subjectId: number | null, grounding: Grounding) =>
    invoke<number>("create_conversation", { subjectId, grounding }),
  conversationMessages: (conversationId: number) =>
    invoke<ChatMessage[]>("conversation_messages", { conversationId }),
  setConversationGrounding: (conversationId: number, grounding: Grounding) =>
    invoke<void>("set_conversation_grounding", { conversationId, grounding }),
  deleteConversation: (conversationId: number) =>
    invoke<void>("delete_conversation", { conversationId }),
  /** One full turn. The question is stored before the model is called. */
  askAssistant: (conversationId: number, question: string, attachments: NewAttachment[] = []) =>
    invoke<ChatMessage>("ask_assistant", { conversationId, question, attachments }),
  conversationMarkdown: (conversationId: number) =>
    invoke<string>("conversation_markdown", { conversationId }),

  // --- export / import ---
  exportJson: () => invoke<string>("export_json"),
  exportToFile: () => invoke<string>("export_to_file"),
  importJson: (contents: string) => invoke<ImportReport>("import_json", { contents }),
};

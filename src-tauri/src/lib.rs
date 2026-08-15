//! Application entry point: opens the database, builds the window and menu bar
//! item, registers every command, and starts the once-a-second ticker that keeps
//! the timer honest.

mod commands;
mod db;
mod errors;
mod export;
mod idle;
mod ingest;
mod inbox;
mod models;
mod notifications;
mod provider;
mod resources;
mod ai;
mod biology;
mod ics;
mod library;
mod anki_import;
mod assessments;
mod assistant;
mod capture;
mod cards;
mod scheduler;
mod secrets;
mod settings;
mod streak;
mod subjects;
mod timer;
mod tray;
mod update;
mod util;
mod workspace;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use commands::AppState;

/// How often the ticker wakes up.
const TICK: Duration = Duration::from_secs(1);

/// Write the running session's progress to disk every this many ticks. Every
/// second would be wasteful; never would lose the session on a force-quit.
const PERSIST_EVERY_TICKS: u64 = 15;

/// How often to re-evaluate notification rules, in ticks (= seconds).
///
/// Five minutes. The rules are state-triggered, so sweeping more often would
/// only re-derive the same answer; sweeping less often would let "reviews are
/// due" go unmentioned for most of an evening.
const NOTIFY_EVERY_TICKS: u64 = 300;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Opens URLs in the user's default browser. Used for the release page
        // when an update is available, and for VCAA links later.
        .plugin(tauri_plugin_opener::init())
        // Native file and folder pickers, for adding material.
        .plugin(tauri_plugin_dialog::init())
        // Bridges macOS UNUserNotificationCenter. Onboarding screen 3 requests
        // permission through this; Pomodoro phase changes use it here.
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // ---- Database -------------------------------------------------
            // `app_data_dir()` resolves to
            //   ~/Library/Application Support/com.armankundu.retain
            // derived from the bundle identifier in tauri.conf.json.
            let data_dir = handle.path().app_data_dir()?;
            let connection = db::open(&data_dir)?;

            // Snapshot on every launch. Cheap, and it means yesterday's database
            // is always recoverable. See docs/icloud-sqlite-analysis.md.
            if let Err(e) = db::snapshot(&connection, &data_dir) {
                eprintln!("[retain] could not write startup snapshot: {e}");
            }

            // Bring freeze accounting up to date for any days that passed while
            // the app was closed.
            if let Ok(threshold) = settings::focused_session_minutes(&connection) {
                if let Err(e) = streak::reconcile(&connection, threshold) {
                    eprintln!("[retain] streak reconcile failed: {e}");
                }
            }

            let state = AppState {
                db: Arc::new(Mutex::new(connection)),
                timer: Arc::new(Mutex::new(None)),
                tray: Mutex::new(None),
            };

            // ---- Menu bar -------------------------------------------------
            let handles = tray::build(&handle)?;
            *state.tray.lock().unwrap() = Some(handles);

            // `manage` hands ownership to Tauri, which passes it to every command.
            app.manage(state);

            // ---- Quick capture --------------------------------------------
            if let Err(e) = register_capture_shortcut(&handle) {
                // A hotkey clash must not stop the app booting — capture is one
                // feature, not the product. The window is still reachable from
                // the UI, and Settings explains what happened.
                eprintln!("[retain] could not register ⌘⇧Space: {e}");
            }

            // ---- Ticker ---------------------------------------------------
            spawn_ticker(handle.clone());
            spawn_update_check(handle);

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it rather than quitting. The whole point
            // of the menu bar timer is that the app keeps counting with no window
            // open; quitting on close would stop sessions by accident.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::complete_onboarding,
            commands::get_setting,
            commands::set_setting,
            commands::set_rest_days,
            commands::list_subjects,
            commands::create_subject,
            commands::update_subject,
            commands::archive_subject,
            commands::unarchive_subject,
            commands::reorder_subjects,
            commands::set_weekly_goal,
            commands::start_timer,
            commands::pause_timer,
            commands::resume_timer,
            commands::stop_timer,
            commands::get_timer,
            commands::set_session_note,
            commands::recent_sessions,
            commands::get_grid,
            commands::get_streak,
            commands::get_weekly_rings,
            commands::preview_card_import,
            commands::import_cards,
            commands::review_queue,
            commands::review_counts,
            commands::answer_card,
            commands::review_forecast,
            commands::error_categories,
            commands::command_words,
            commands::create_error_entry,
            commands::list_error_entries,
            commands::delete_error_entry,
            commands::error_entry_image,
            commands::due_error_reattempts,
            commands::start_error_reattempt,
            commands::commit_error_reattempt,
            commands::reveal_error_answer,
            commands::assess_error_reattempt,
            commands::recurring_errors,
            commands::preview_capture,
            commands::save_capture,
            commands::hide_capture_window,
            commands::list_inbox,
            commands::inbox_count,
            commands::triage_capture_to_task,
            commands::triage_capture,
            commands::list_tasks,
            commands::set_task_done,
            commands::delete_task,
            commands::create_assessment,
            commands::list_assessments,
            commands::delete_assessment,
            commands::log_topic_review,
            commands::surface_topics,
            commands::create_topic,
            commands::list_topics,
            commands::delete_topic,
            commands::notification_settings,
            commands::set_notification_settings,
            commands::notifications_sent_today,
            commands::preview_notifications,
            // AI — every one of these degrades to "add a key" rather than
            // failing, and nothing else in the app depends on them.
            commands::ai_status,
            commands::ai_set_provider,
            commands::ai_set_model,
            commands::ai_task_from_note,
            commands::ai_cards_from_notes,
            commands::ai_weekly_review,
            commands::weekly_facts,
            commands::ai_practice_question,
            commands::ai_suggest_category,
            commands::discard_session,
            commands::day_detail,
            commands::preview_intervals,
            // Resources and the saved-output library.
            commands::list_resources,
            commands::add_resource,
            commands::delete_resource,
            commands::search_resources,
            commands::list_library,
            commands::set_library_pinned,
            commands::rename_library_item,
            commands::delete_library_item,
            commands::library_item_markdown,
            commands::export_library_item,
            commands::ai_notes,
            // Subject folders, folder import, and the assistant.
            commands::ensure_subject_folders,
            commands::reveal_folder,
            commands::import_folder,
            commands::read_file_text,
            commands::list_conversations,
            commands::create_conversation,
            commands::conversation_messages,
            commands::set_conversation_grounding,
            commands::delete_conversation,
            commands::ask_assistant,
            commands::conversation_markdown,
            commands::list_ai_models,
            commands::test_ai_model,
            // Calendar — ICS subscription only.
            commands::calendar_status,
            commands::set_calendar_settings,
            commands::sync_calendar,
            commands::upcoming_events,
            commands::clear_calendar,
            // Biology 3/4. No study-design content ships in the binary; the
            // topic tree is filled from the user's own outline.
            commands::topic_tree,
            commands::preview_topic_outline,
            commands::import_topic_outline,
            commands::terminology_summary,
            commands::exam_state,
            commands::start_exam,
            commands::set_exam_paused,
            commands::finish_exam,
            commands::cancel_exam,
            commands::score_exam,
            commands::exam_history,
            // Update check — reports only; never downloads or installs.
            commands::update_status,
            commands::check_for_updates,
            commands::open_release_page,
            commands::secret_set,
            commands::secret_verify_and_store,
            commands::secret_store_unverified,
            commands::secret_test_stored,
            commands::secret_has,
            commands::secret_delete,
            commands::export_json,
            commands::export_to_file,
            commands::import_json,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Retain");
}

/// Register ⌘⇧Space and wire it to the capture window.
///
/// The window is created hidden at startup by `tauri.conf.json` and never
/// destroyed, so the hotkey path is `show()` + `set_focus()` — no window
/// construction, no webview boot, no fade. That pre-warming is what makes
/// capture usable mid-class; building a window on the hot path would cost
/// hundreds of milliseconds and the habit would die.
///
/// The shortcut is fixed at ⌘⇧Space per the brief. There is deliberately no
/// rebinding UI — that's a settings subsystem the brief didn't ask for, and it
/// would only earn its place if the fixed binding actually collided.
fn register_capture_shortcut(app: &tauri::AppHandle) -> anyhow::Result<()> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

    let combo = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);

    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, pressed, event| {
                // Only act on key-down. Without this the window toggles twice
                // per press — once down, once up — and appears to do nothing.
                if pressed == &combo && event.state() == ShortcutState::Pressed {
                    toggle_capture_window(app);
                }
            })
            .build(),
    )?;

    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut().register(combo)?;
    Ok(())
}

/// Show the capture window, or hide it if it's already up.
fn toggle_capture_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("capture") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    // Re-centre before showing: the user may have moved to a different display
    // or changed resolution since the window was created at launch.
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to("capture", "capture:opened", ());
}

/// The heartbeat.
///
/// Runs on its own OS thread so it keeps going regardless of what the UI is
/// doing — including when there is no UI, because the window is closed.
///
/// Each tick it:
///   1. asks macOS how long the machine has been idle, and pauses or resumes,
///   2. advances the Pomodoro cycle and notifies on a phase change,
///   3. periodically writes progress to the database,
///   4. repaints the menu bar,
///   5. emits the state to the window, if one is open.
fn spawn_ticker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut ticks: u64 = 0;
        // The last time this loop actually ran. Compared against the clock each
        // tick to notice that the process was suspended — see
        // `timer::detect_suspension`.
        let mut last_tick = chrono::Utc::now();

        loop {
            std::thread::sleep(TICK);
            ticks += 1;

            let now = chrono::Utc::now();
            let previous_tick = last_tick;
            last_tick = now;
            let suspension = timer::detect_suspension(previous_tick, now);

            let state = app.state::<AppState>();

            // Lock order is timer-then-database, matching commands.rs. Both locks
            // are released at the end of this block, before the tray update.
            let snapshot = {
                let mut slot = state.timer.lock().expect("timer mutex poisoned");

                if let Some(active) = slot.as_mut() {
                    let conn = state.db.lock().expect("database mutex poisoned");

                    // 0. Did the Mac sleep? This has to run BEFORE idle handling:
                    //    the keypress that woke the machine resets the idle
                    //    counter, so by the time we ask, the idle detector sees
                    //    an active machine and would happily bank the whole
                    //    sleep as study time.
                    if let Some(gap) = suspension {
                        eprintln!(
                            "[retain] resumed after a {gap}s gap (sleep or suspension); \
                             crediting only time up to the last tick"
                        );
                        if let Err(e) = timer::handle_suspension(&conn, active, previous_tick) {
                            eprintln!("[retain] could not close the session across sleep: {e}");
                        }
                    }

                    // 1. Idle handling.
                    let idle_seconds = idle::seconds_since_last_input();
                    if let Err(e) = timer::maybe_auto_pause_or_resume(&conn, active, idle_seconds) {
                        eprintln!("[retain] idle handling failed: {e}");
                    }

                    // 2. Pomodoro phase transitions.
                    match timer::advance_pomodoro(&conn, active) {
                        Ok(timer::PhaseChange::StartedBreak { after_blocks }) => {
                            let minutes = active.break_seconds / 60;
                            notify(
                                &app,
                                &format!("Block {after_blocks} done"),
                                &format!(
                                    "{} — {minutes} minute break. The timer picks itself back up.",
                                    active.subject_name
                                ),
                            );
                        }
                        Ok(timer::PhaseChange::StartedWork) => {
                            let minutes = active.work_seconds / 60;
                            notify(
                                &app,
                                "Back to it",
                                &format!("{} — {minutes} minutes.", active.subject_name),
                            );
                        }
                        Ok(timer::PhaseChange::None) => {}
                        Err(e) => eprintln!("[retain] pomodoro advance failed: {e}"),
                    }

                    // 3. Periodic durability.
                    if ticks.is_multiple_of(PERSIST_EVERY_TICKS) {
                        if let Err(e) =
                            timer::persist_progress(&conn, active, chrono::Utc::now())
                        {
                            eprintln!("[retain] could not persist session progress: {e}");
                        }
                    }
                }

                commands::snapshot_of(&slot)
            };

            // 4. Menu bar.
            if let Ok(guard) = state.tray.lock() {
                if let Some(handles) = guard.as_ref() {
                    tray::update(handles, snapshot.as_ref());
                }
            }

            // 4b. Notification sweep. State-triggered: this asks the rules what
            //     is true right now, and they answer with nothing most of the
            //     time. Only fires while the app is running — see the module
            //     docs on `notifications` for that limitation.
            if ticks.is_multiple_of(NOTIFY_EVERY_TICKS) {
                let conn = state.db.lock().expect("database mutex poisoned");
                let now = chrono::Utc::now();
                match notifications::pending(&conn, now) {
                    Ok(due) => {
                        for candidate in due {
                            notify(&app, &candidate.title, &candidate.body);
                            if let Err(e) = notifications::record(&conn, &candidate, now) {
                                eprintln!("[retain] could not record notification: {e}");
                            }
                        }
                    }
                    Err(e) => eprintln!("[retain] notification sweep failed: {e}"),
                }
            }

            // 5. Tell the window, if it is listening. `emit` is a no-op when no
            // window is open, so this costs nothing while running headless.
            let _ = app.emit("timer:tick", &snapshot);
        }
    });
}

/// Fire a notification, ignoring failure.
///
/// A notification that doesn't appear — because permission was declined, which is
/// entirely the user's right — must never take the timer down with it.
fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Ask GitHub whether there's a newer release — at most once a day, always in
/// the background, and never in a way that can delay or fail startup.
///
/// The result is written to settings and read by the Settings screen. Nothing
/// is downloaded or installed, and no dialog interrupts anyone: a version you
/// didn't ask about is a notice, not an event.
fn spawn_update_check(app: tauri::AppHandle) {
    // A plain thread for the delay, so a first launch spends its startup on the
    // window rather than on a network request.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));

        // The connection handle is cloned out of app state up front. Holding a
        // `State` borrow across the await below would not compile, and cloning
        // the Arc is the same handle either way.
        let db = app.state::<commands::AppState>().db.clone();

        // Scoped so the guard is released before any network work begins.
        let stale = {
            let Ok(conn) = db.lock() else { return };
            update::should_check(&conn, chrono::Utc::now()).unwrap_or(false)
        };
        if !stale {
            return;
        }

        tauri::async_runtime::spawn(async move {
            let status = update::check(env!("CARGO_PKG_VERSION")).await;

            if let Ok(conn) = db.lock() {
                // A failure here is genuinely nothing: the stored status stays
                // as it was and the app carries on.
                let _ = update::store(&conn, &status, chrono::Utc::now());
            }
        });
    });
}

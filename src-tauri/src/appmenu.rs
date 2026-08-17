//! The menu bar at the top of the screen.
//!
//! Retain had the default one, which on macOS is a File menu containing "Close
//! Window" and nothing else. That is worse than no menu: it tells you the app
//! has menus, then has nothing in them, and it means ⌘N, ⌘P and ⌘, do nothing
//! in an app where all three have obvious meanings.
//!
//! Two things this is for, and only two:
//!
//!   * **Keyboard shortcuts.** A menu item is how macOS learns a shortcut
//!     exists. Registering ⌘⇧N as a global hotkey would steal it from every
//!     other app; as a menu item it works when Retain is frontmost, which is
//!     what you want for "new sticky".
//!   * **Discoverability.** The View menu lists every screen with its shortcut,
//!     which is the only place in the app they're written down.
//!
//! Edit is built entirely from predefined items. Cut, copy, paste and undo have
//! to be real system items or they don't work in a text field at all — a
//! hand-rolled "Copy" that emits an event to the frontend cannot copy the
//! selection out of a native input.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Build the whole menu.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_menu = Submenu::with_items(
        app,
        "Retain",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Retain"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "go:settings", "Settings…", true, Some("Cmd+,"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "new:note", "New Note", true, Some("Cmd+N"))?,
            &MenuItem::with_id(app, "new:sticky", "New Sticky Note", true, Some("Cmd+Shift+N"))?,
            &PredefinedMenuItem::separator(app)?,
            // The global ⌘⇧Space still works from any app; this is the same
            // action for when Retain is already in front.
            &MenuItem::with_id(app, "capture", "Quick Capture", true, Some("Cmd+Shift+Space"))?,
            &MenuItem::with_id(app, "screenshot", "Screenshot into Note", true, Some("Cmd+Shift+4"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "print", "Print…", true, Some("Cmd+P"))?,
            &MenuItem::with_id(app, "export", "Export All Data…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Close Window"))?,
        ],
    )?;

    // Every item predefined. A hand-written Copy cannot reach the selection
    // inside a native text field, so these have to be the system's own.
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    // The only written-down list of the app's shortcuts.
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "go:today", "Today", true, Some("Cmd+1"))?,
            &MenuItem::with_id(app, "go:timer", "Timer", true, Some("Cmd+2"))?,
            &MenuItem::with_id(app, "go:week", "Week", true, Some("Cmd+3"))?,
            &MenuItem::with_id(app, "go:review", "Review", true, Some("Cmd+4"))?,
            &MenuItem::with_id(app, "go:notes", "Notes", true, Some("Cmd+5"))?,
            &MenuItem::with_id(app, "go:library", "Library", true, Some("Cmd+6"))?,
            &MenuItem::with_id(app, "go:assistant", "Assistant", true, Some("Cmd+7"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "go:inbox", "Inbox", true, None::<&str>)?,
            &MenuItem::with_id(app, "go:errors", "Error Log", true, None::<&str>)?,
            &MenuItem::with_id(app, "go:assessments", "Assessments", true, None::<&str>)?,
            &MenuItem::with_id(app, "go:progress", "Progress", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let timer = Submenu::with_items(
        app,
        "Timer",
        true,
        &[
            &MenuItem::with_id(app, "timer:toggle", "Pause or Resume", true, Some("Cmd+Shift+P"))?,
            &MenuItem::with_id(app, "timer:stop", "Stop Session", true, Some("Cmd+Shift+."))?,
        ],
    )?;

    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "go:main", "Retain", true, Some("Cmd+0"))?,
        ],
    )?;

    let help = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &MenuItem::with_id(app, "help:shortcuts", "Keyboard Shortcuts", true, Some("Cmd+/"))?,
            &MenuItem::with_id(app, "help:repo", "Retain on GitHub", true, None::<&str>)?,
            &MenuItem::with_id(app, "help:updates", "Check for Updates…", true, None::<&str>)?,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &timer, &window, &help])
}

/// Act on a menu selection.
///
/// Navigation is emitted to the frontend rather than performed here — the
/// router lives in React, and a Rust-side copy of which screens exist would be
/// one more thing to keep in step with the sidebar.
pub fn on_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "new:sticky" => crate::commands::tray_new_sticky_generic(app),
        "capture" => crate::show_capture_from_menu(app),
        "help:repo" => {
            use tauri_plugin_opener::OpenerExt;
            let _ = app.opener().open_url(crate::update::releases_page(), None::<&str>);
        }
        "go:main" => crate::tray::show_main_window_generic(app),
        "timer:toggle" | "timer:stop" | "print" | "export" | "screenshot" | "new:note"
        | "help:shortcuts" | "help:updates" => forward(app, id),
        // Everything beginning `go:` is a route change.
        other if other.starts_with("go:") => forward(app, other),
        _ => {}
    }
}

/// Hand the selection to whichever window is in front.
///
/// Sent to every window rather than just `main`: a sticky is a window too, and
/// ⌘P in a focused sticky should print that sticky.
fn forward<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let _ = app.emit("menu", id);
    if let Some(main) = app.get_webview_window("main") {
        // A route change with no window to show it in is a no-op the user reads
        // as the menu being broken.
        if id.starts_with("go:") {
            let _ = main.show();
            let _ = main.set_focus();
        }
    }
}

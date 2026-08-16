//! The macOS menu bar item.
//!
//! This is Tauri core, not a plugin — the `tray-icon` feature in Cargo.toml is
//! all it takes. The brief's requirement is "I must never need to open the app to
//! check", which `TrayIcon::set_title` delivers: on macOS a tray icon can carry
//! text beside it, and we rewrite that text once a second from the ticker thread.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::models::{PauseReason, TimerSnapshot};
use crate::util::format_clock;

/// Menu item handles we keep so their text can be updated in place, rather than
/// rebuilding the whole menu every second.
pub struct TrayHandles {
    pub icon: TrayIcon<Wry>,
    pub status: MenuItem<Wry>,
    pub pause_resume: MenuItem<Wry>,
    pub stop: MenuItem<Wry>,
}

/// Draw the menu bar glyph: a thin ring, like a clock face without hands.
///
/// Building the pixels here rather than shipping a .png keeps the asset pipeline
/// empty and means there is no file to go missing from a bundle. macOS is told to
/// treat it as a *template* image, which means it ignores our colours entirely and
/// uses only the alpha channel, tinting the shape to match the menu bar — so it
/// comes out correct in light mode, dark mode, and when the menu bar is
/// translucent over a bright wallpaper.
fn menu_bar_icon() -> Image<'static> {
    // 36×36 covers a Retina menu bar; macOS scales it to the bar's height.
    const SIZE: i32 = 36;
    const OUTER: f32 = 15.0;
    const THICKNESS: f32 = 3.0;

    let centre = (SIZE as f32 - 1.0) / 2.0;
    let inner = OUTER - THICKNESS;

    // RGBA, four bytes per pixel.
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let distance = (dx * dx + dy * dy).sqrt();

            // Coverage falls off over one pixel at each edge of the ring, which
            // is enough to keep the curve from looking jagged.
            let outer_edge = (OUTER - distance).clamp(0.0, 1.0);
            let inner_edge = (distance - inner).clamp(0.0, 1.0);
            let alpha = (outer_edge * inner_edge * 255.0) as u8;

            // Black with variable alpha. The colour is irrelevant under a
            // template image, but black is the convention.
            pixels.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }

    Image::new_owned(pixels, SIZE as u32, SIZE as u32)
}

/// Build the tray icon and its menu. Called once during app setup.
pub fn build(app: &AppHandle) -> anyhow::Result<TrayHandles> {
    // `with_id` gives each item a stable name we match on in the click handler.
    let status = MenuItem::with_id(app, "status", "No session running", false, None::<&str>)?;
    let pause_resume = MenuItem::with_id(app, "pause_resume", "Pause", false, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop session", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Retain", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Retain", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &pause_resume,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &quit,
        ],
    )?;

    let icon = TrayIconBuilder::with_id("retain-tray")
        .icon(menu_bar_icon())
        .icon_as_template(true)
        .menu(&menu)
        // Left click opens the app; the menu is on right click. Clicking a status
        // item you use as a glanceable clock should do the obvious thing.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let app = app.clone();
            match event.id().as_ref() {
                "open" => show_main_window(&app),
                "quit" => app.exit(0),
                "pause_resume" => crate::commands::tray_toggle_pause(&app),
                "stop" => crate::commands::tray_stop(&app),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(TrayHandles {
        icon,
        status,
        pause_resume,
        stop,
    })
}

pub fn show_main_window(app: &AppHandle) {
    // Closing the window hides the whole application, so the app itself has to
    // come back before its window can. Showing the window alone leaves it
    // hidden behind whatever you switched to.
    #[cfg(target_os = "macos")]
    let _ = app.show();

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Push the current timer state into the menu bar. Called once a second.
pub fn update(handles: &TrayHandles, snapshot: Option<&TimerSnapshot>) {
    match snapshot {
        Some(s) => {
            // The title is the live clock. Active time, not wall clock — the menu
            // bar should agree with what the session will actually be worth.
            let clock = format_clock(s.active_seconds);

            // The word "paused" spelled out took more menu-bar width than the
            // clock itself, and the menu bar is the most contested space on the
            // screen. A leading glyph carries the same state in one character:
            // a filled dot is running, a hollow one is stopped, and the reason
            // is in the menu you open to act on it anyway.
            //
            // Space-separated rather than concatenated — macOS renders the
            // title in a proportional font, and the glyph needs the gap to
            // avoid touching the first digit.
            let title = match s.paused_reason {
                Some(PauseReason::Manual) => format!("◦ {clock}"),
                // Idle and break are still *paused*, but not by you. Same
                // hollow glyph, because the distinction that matters in the
                // menu bar is only "is this counting".
                Some(PauseReason::Idle) | Some(PauseReason::Break) => format!("◦ {clock}"),
                None => format!("● {clock}"),
            };
            let _ = handles.icon.set_title(Some(title));

            let topic = s
                .topic_name
                .as_ref()
                .map(|t| format!(" · {t}"))
                .unwrap_or_default();
            // The reason lives here, where there's room for it and where you
            // are when you decide what to do about it.
            let _ = handles.status.set_text(match s.paused_reason {
                Some(PauseReason::Manual) => format!("{}{} — paused", s.subject_name, topic),
                Some(PauseReason::Idle) => {
                    format!("{}{} — paused, you went quiet", s.subject_name, topic)
                }
                Some(PauseReason::Break) => format!("{}{} — on a break", s.subject_name, topic),
                None => format!("{}{}", s.subject_name, topic),
            });

            let _ = handles.pause_resume.set_text(if s.paused_reason.is_some() {
                "Resume"
            } else {
                "Pause"
            });
            let _ = handles.pause_resume.set_enabled(true);
            let _ = handles.stop.set_enabled(true);
        }
        None => {
            // No session: drop back to just the glyph. An idle "0:00" in the menu
            // bar reads as a broken timer.
            let _ = handles.icon.set_title(None::<&str>);
            let _ = handles.status.set_text("No session running");
            let _ = handles.pause_resume.set_text("Pause");
            let _ = handles.pause_resume.set_enabled(false);
            let _ = handles.stop.set_enabled(false);
        }
    }
}

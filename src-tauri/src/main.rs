// Prevents a second, empty console window from opening alongside the app on
// Windows release builds. Harmless on macOS, and kept so the file matches what
// every Tauri project expects.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    retain_lib::run()
}

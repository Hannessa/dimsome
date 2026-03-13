#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Hand off startup to the shared library so tests and app entry stay aligned.
    dimsome_tauri_lib::run();
}

mod commands;
mod hotkeys;
mod models;
mod schedule;
mod settings;
mod startup;
mod state;
mod windows;

use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    Emitter, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::{
    hotkeys::{HotkeyAction, HotkeyManager},
    state::{initialize_state, refresh_state, reset_dimming_before_exit, start_loop},
};

const SETTINGS_WINDOW_WIDTH: f64 = 896.0;
const SETTINGS_WINDOW_HEIGHT: f64 = 602.0;
const SETTINGS_WINDOW_BACKGROUND: Color = Color(0x11, 0x11, 0x11, 0xFF);
const TRAY_ICON_WIDTH: u32 = 32;
const TRAY_ICON_HEIGHT: u32 = 32;
const TRAY_ICON_RGBA: &[u8] = include_bytes!("../icons/tray-icon.rgba");

fn bottom_right_work_area_position(
    work_area: PhysicalRect<i32, u32>,
    window_size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    // Start from the work area's bottom-right corner so the full native frame clears the taskbar.
    let right_aligned_x =
        work_area.position.x + work_area.size.width as i32 - window_size.width as i32;
    let bottom_aligned_y =
        work_area.position.y + work_area.size.height as i32 - window_size.height as i32;

    // Clamp oversized windows back into the visible work area instead of letting them drift off-screen.
    let clamped_x = right_aligned_x.max(work_area.position.x);
    let clamped_y = bottom_aligned_y.max(work_area.position.y);

    (clamped_x, clamped_y).into()
}

fn position_settings_window(window: &WebviewWindow) -> tauri::Result<()> {
    // Use the primary monitor so the tray-opened panel matches the notification area display.
    let Some(monitor) = window.primary_monitor()? else {
        return Ok(());
    };

    // Read the outer frame size so the full window aligns with the work area edges.
    let window_size = window.outer_size()?;
    let work_area = *monitor.work_area();

    // Align the hidden window before first show so it appears in the expected corner immediately.
    let position = bottom_right_work_area_position(work_area, window_size);
    window.set_position(position)?;
    Ok(())
}

fn configure_settings_window(window: &WebviewWindow) {
    let settings_window = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            // Hide to tray instead of destroying the window on close.
            api.prevent_close();
            let _ = settings_window.hide();
        }
    });
}

pub(crate) fn ensure_settings_window(app: &tauri::AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("settings") {
        return Ok(window);
    }

    // Lazily create the settings window so the tray app can start hidden.
    let window = WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("index.html?window=settings".into()),
    )
    .title("Dimsome")
    .inner_size(SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT)
    // Paint the first native frame dark before the webview finishes loading.
    .background_color(SETTINGS_WINDOW_BACKGROUND)
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .visible(false)
    .build()?;

    // Place the first session open in the primary work area's bottom-right corner.
    if let Err(error) = position_settings_window(&window) {
        // Keep the settings window usable even if placement metrics are unavailable.
        eprintln!("Failed to position settings window: {error}");
    }

    configure_settings_window(&window);
    Ok(window)
}

pub(crate) fn open_settings_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = ensure_settings_window(app)?;
    window.show()?;
    window.set_focus()?;
    Ok(())
}

pub(crate) fn toggle_settings_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = ensure_settings_window(app)?;

    // Hide the panel when it is already visible so tray clicks alternate between show and hide.
    if window.is_visible()? {
        window.hide()?;
        return Ok(());
    }

    // Show and focus the panel when it is currently tucked away in the tray.
    window.show()?;
    window.set_focus()?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(initialize_state())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::get_dimming_capabilities,
            commands::save_settings,
            commands::get_effective_state,
            commands::apply_manual_dim,
            commands::pause_schedule,
            commands::resume_schedule,
            commands::get_startup_state,
            commands::set_startup_enabled,
            commands::exit_app
        ])
        .setup(|app| {
            let shared_for_hotkeys = app.state::<crate::state::SharedState>().inner().clone();
            let shared_for_hotkeys_handler = shared_for_hotkeys.clone();
            let app_handle_for_hotkeys = app.handle().clone();
            let hotkey_manager = HotkeyManager::new(move |action| {
                let shared = shared_for_hotkeys_handler.clone();
                let app_handle = app_handle_for_hotkeys.clone();
                tauri::async_runtime::spawn(async move {
                    // Map global hotkeys onto the same nudge logic the UI uses.
                    match action {
                        HotkeyAction::DimMore => {
                            let _ = crate::state::nudge(&shared, &app_handle, 1.0).await;
                        }
                        HotkeyAction::DimLess => {
                            let _ = crate::state::nudge(&shared, &app_handle, -1.0).await;
                        }
                    }
                });
            });
            let initial_hotkeys = shared_for_hotkeys
                .blocking_read()
                .settings
                .manual_hotkeys
                .clone();
            hotkey_manager.update_bindings(initial_hotkeys);
            app.manage(hotkey_manager);

            // Build the small tray menu around the app's most common actions.
            let open_settings =
                MenuItemBuilder::with_id("open_settings", "Open Settings").build(app)?;
            let pause_resume =
                MenuItemBuilder::with_id("pause_resume", "Pause / Resume").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Exit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&open_settings, &pause_resume, &quit])
                .build()?;
            let tray_icon = Image::new(TRAY_ICON_RGBA, TRAY_ICON_WIDTH, TRAY_ICON_HEIGHT);

            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                // Keep the native tray menu on right click only so left click can open Settings.
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open_settings" => {
                        if let Err(error) = open_settings_window(app) {
                            eprintln!("Failed to open settings window: {error}");
                        }
                    }
                    "pause_resume" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(shared) =
                                app_handle.try_state::<crate::state::SharedState>()
                            {
                                let current = shared.inner().read().await.current_state.clone();
                                if current.mode == crate::models::EffectiveDimMode::Paused {
                                    let _ = crate::state::resume(shared.inner(), &app_handle).await;
                                } else {
                                    let _ = crate::state::pause(shared.inner(), &app_handle).await;
                                }
                            }
                        });
                    }
                    "quit" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(shared) =
                                app_handle.try_state::<crate::state::SharedState>()
                            {
                                reset_dimming_before_exit(shared.inner()).await;
                            }
                            app_handle.exit(0);
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        // Toggle on the single click-up event so rapid clicks do not also retrigger on double click.
                        if let Err(error) = toggle_settings_window(tray.app_handle()) {
                            eprintln!("Failed to toggle settings window: {error}");
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            let shared = app.state::<crate::state::SharedState>().inner().clone();
            let shared_for_refresh = shared.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Refresh once immediately so the dimming engine applies startup state.
                let _ = refresh_state(&shared_for_refresh, Some(&app_handle)).await;
            });
            start_loop(shared, app.handle().clone());
            let _ = app.emit("startup_state_changed", crate::startup::get_startup_state());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Dimsome Tauri");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_area(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect<i32, u32> {
        PhysicalRect {
            position: (x, y).into(),
            size: (width, height).into(),
        }
    }

    fn window_size(width: u32, height: u32) -> PhysicalSize<u32> {
        (width, height).into()
    }

    #[test]
    fn positions_window_in_bottom_right_of_standard_work_area() {
        let position =
            bottom_right_work_area_position(work_area(0, 0, 1920, 1040), window_size(896, 602));

        assert_eq!(position.x, 1024);
        assert_eq!(position.y, 438);
    }

    #[test]
    fn positions_window_in_bottom_right_of_offset_work_area() {
        let position =
            bottom_right_work_area_position(work_area(-1280, 0, 1280, 984), window_size(896, 602));

        assert_eq!(position.x, -896);
        assert_eq!(position.y, 382);
    }

    #[test]
    fn clamps_oversized_window_to_work_area_origin() {
        let position =
            bottom_right_work_area_position(work_area(100, 200, 700, 500), window_size(896, 602));

        assert_eq!(position.x, 100);
        assert_eq!(position.y, 200);
    }
}

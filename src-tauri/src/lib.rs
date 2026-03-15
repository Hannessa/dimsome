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
    menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    Emitter, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent, Wry,
};

use crate::{
    hotkeys::{HotkeyAction, HotkeyManager},
    schedule::clamp_dim_precise,
    state::{initialize_state, refresh_state, reset_dimming_before_exit, start_loop},
};

const APP_NAME: &str = "Dimsome";
const SETTINGS_WINDOW_WIDTH: f64 = 896.0;
const SETTINGS_WINDOW_HEIGHT: f64 = 602.0;
const SETTINGS_WINDOW_BACKGROUND: Color = Color(0x11, 0x11, 0x11, 0xFF);
pub(crate) const TRAY_ICON_ID: &str = "main";
const TRAY_ICON_WIDTH: u32 = 32;
const TRAY_ICON_HEIGHT: u32 = 32;
const TRAY_ICON_RGBA: &[u8] = include_bytes!("../icons/tray-icon.rgba");

struct TrayMenuState(Menu<Wry>);

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

const TRAY_BRIGHTNESS_PRESETS: [u8; 10] = [100, 90, 80, 70, 60, 50, 40, 30, 20, 10];

fn tray_brightness_menu_id(brightness_percent: u8) -> String {
    format!("brightness_{brightness_percent}")
}

fn brightness_percent_from_dim(dim_percent: f64) -> u8 {
    // Convert the stored dim percentage back into the brightness label shown to the user.
    let clamped_dim_percent = clamp_dim_precise(dim_percent).clamp(0.0, 99.0);
    (100.0 - clamped_dim_percent).round().clamp(1.0, 100.0) as u8
}

fn checked_tray_brightness_preset(dim_percent: f64) -> u8 {
    // Convert the live dim level into a raw brightness value before picking a preset bucket.
    let brightness_percent = (100.0 - clamp_dim_precise(dim_percent).clamp(0.0, 99.0)).floor() as u8;

    // Keep the lowest tray preset selected for all very dark values below twenty percent.
    if brightness_percent < 20 {
        return 10;
    }

    // Preserve the full-brightness shortcut as its own exact checked state.
    if brightness_percent >= 100 {
        return 100;
    }

    // Round down to the nearest ten so the menu reflects the current preset bucket.
    (brightness_percent / 10) * 10
}

pub(crate) fn format_tray_tooltip(dim_percent: f64) -> String {
    // Keep the tray hover text focused on the app name first, with the live brightness alongside it.
    format!("{APP_NAME} - {}%", brightness_percent_from_dim(dim_percent))
}

pub(crate) fn sync_tray_tooltip(app: &tauri::AppHandle, dim_percent: f64) {
    let tooltip = format_tray_tooltip(dim_percent);

    // Look up the registered tray by its stable id so every state change updates the same icon.
    let Some(tray) = app.tray_by_id(TRAY_ICON_ID) else {
        eprintln!("Failed to update tray tooltip: tray icon '{TRAY_ICON_ID}' was not found");
        return;
    };

    // Keep tooltip failures non-fatal so dimming still works even if the shell rejects an update.
    if let Err(error) = tray.set_tooltip(Some(&tooltip)) {
        eprintln!("Failed to update tray tooltip: {error}");
    }
}

pub(crate) fn sync_tray_menu(
    app: &tauri::AppHandle,
    dim_percent: f64,
    schedule_enabled: bool,
) {
    let Some(menu) = app.try_state::<TrayMenuState>() else {
        eprintln!("Failed to update tray menu: managed tray menu state was not found");
        return;
    };

    let checked_preset = checked_tray_brightness_preset(dim_percent);

    // Keep exactly one brightness preset checked so the menu mirrors the live output.
    for brightness_percent in TRAY_BRIGHTNESS_PRESETS {
        let Some(item) = menu.0.get(&tray_brightness_menu_id(brightness_percent)) else {
            eprintln!("Failed to update tray menu: brightness item '{brightness_percent}%' was not found");
            continue;
        };

        let Some(check_item) = item.as_check_menuitem() else {
            eprintln!("Failed to update tray menu: brightness item '{brightness_percent}%' is not checkable");
            continue;
        };

        if let Err(error) = check_item.set_checked(brightness_percent == checked_preset) {
            eprintln!("Failed to update tray menu brightness checkmark: {error}");
        }
    }

    // Mirror the persisted schedule toggle into the tray so it stays in sync with the app.
    let Some(item) = menu.0.get("enable_schedule") else {
        eprintln!("Failed to update tray menu: schedule item was not found");
        return;
    };

    let Some(check_item) = item.as_check_menuitem() else {
        eprintln!("Failed to update tray menu: schedule item is not checkable");
        return;
    };

    if let Err(error) = check_item.set_checked(schedule_enabled) {
        eprintln!("Failed to update tray menu schedule checkmark: {error}");
    }
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
            commands::apply_manual_dim_and_disable_schedule,
            commands::enable_schedule,
            commands::toggle_schedule_enabled,
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
                            // Route global hotkeys through the hotkey-specific floor logic.
                            if let Err(error) =
                                crate::state::nudge_hotkey(&shared, &app_handle, 1.0).await
                            {
                                eprintln!("Failed to apply dim-more hotkey: {error}");
                            }
                        }
                        HotkeyAction::DimLess => {
                            // Route global hotkeys through the hotkey-specific floor logic.
                            if let Err(error) =
                                crate::state::nudge_hotkey(&shared, &app_handle, -1.0).await
                            {
                                eprintln!("Failed to apply dim-less hotkey: {error}");
                            }
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
            let brightness_items = TRAY_BRIGHTNESS_PRESETS
                .iter()
                .map(|brightness_percent| {
                    // Keep the native labels in brightness terms while the backend still stores dim percentage.
                    CheckMenuItemBuilder::with_id(
                        tray_brightness_menu_id(*brightness_percent),
                        format!("{brightness_percent}%"),
                    )
                    .checked(*brightness_percent == 100)
                    .build(app)
                })
                .collect::<tauri::Result<Vec<_>>>()?;
            let enable_schedule = CheckMenuItemBuilder::with_id("enable_schedule", "Enable Schedule")
                .checked(true)
                .build(app)?;
            let open_settings =
                MenuItemBuilder::with_id("open_settings", "Open Settings").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Exit").build(app)?;
            let schedule_separator = PredefinedMenuItem::separator(app)?;
            let actions_separator = PredefinedMenuItem::separator(app)?;
            let mut menu = MenuBuilder::new(app);

            // Put the fixed brightness shortcuts first so right-click access is direct.
            for item in &brightness_items {
                menu = menu.item(item);
            }

            // Break the menu into visual groups so the fixed presets read separately from app actions.
            let menu = menu
                .item(&schedule_separator)
                .item(&enable_schedule)
                .item(&actions_separator)
                .item(&open_settings)
                .item(&quit)
                .build()?;
            app.manage(TrayMenuState(menu.clone()));
            let tray_icon = Image::new(TRAY_ICON_RGBA, TRAY_ICON_WIDTH, TRAY_ICON_HEIGHT);

            let _tray = TrayIconBuilder::with_id(TRAY_ICON_ID)
                .icon(tray_icon)
                // Seed the hover text immediately so the tray icon never appears unnamed at startup.
                .tooltip(format_tray_tooltip(0.0))
                .menu(&menu)
                // Keep the native tray menu on right click only so left click can open Settings.
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if let Some(brightness_percent) = event
                        .id
                        .as_ref()
                        .strip_prefix("brightness_")
                        .and_then(|value| value.parse::<f64>().ok())
                    {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(shared) =
                                app_handle.try_state::<crate::state::SharedState>()
                            {
                                // Convert brightness labels into the stored dim percentage used by the engine.
                                let dim_percent = (100.0 - brightness_percent).clamp(0.0, 99.0);
                                if let Err(error) = crate::state::apply_manual_dim_and_disable_schedule(
                                    shared.inner(),
                                    &app_handle,
                                    dim_percent,
                                )
                                .await
                                {
                                    eprintln!("Failed to apply tray brightness preset: {error}");
                                }
                            }
                        });
                        return;
                    }

                    match event.id.as_ref() {
                        "enable_schedule" => {
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Some(shared) =
                                    app_handle.try_state::<crate::state::SharedState>()
                                {
                                    if let Err(error) = crate::state::toggle_schedule_enabled(
                                        shared.inner(),
                                        &app_handle,
                                    )
                                    .await
                                    {
                                        eprintln!("Failed to toggle schedule from tray: {error}");
                                    }
                                }
                            });
                        }
                        "open_settings" => {
                            if let Err(error) = open_settings_window(app) {
                                eprintln!("Failed to open settings window: {error}");
                            }
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
                    }
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

    #[test]
    fn formats_tray_tooltip_for_full_brightness() {
        assert_eq!(format_tray_tooltip(0.0), "Dimsome - 100%");
    }

    #[test]
    fn formats_tray_tooltip_for_half_brightness() {
        assert_eq!(format_tray_tooltip(50.0), "Dimsome - 50%");
    }

    #[test]
    fn formats_tray_tooltip_for_near_minimum_brightness() {
        assert_eq!(format_tray_tooltip(99.0), "Dimsome - 1%");
    }

    #[test]
    fn rounds_fractional_brightness_labels_for_tray_tooltip() {
        assert_eq!(format_tray_tooltip(33.6), "Dimsome - 66%");
    }

    #[test]
    fn checks_full_brightness_preset_at_one_hundred_percent() {
        assert_eq!(checked_tray_brightness_preset(0.0), 100);
    }

    #[test]
    fn checks_ninety_percent_preset_for_ninety_nine_percent_brightness() {
        assert_eq!(checked_tray_brightness_preset(1.0), 90);
    }

    #[test]
    fn checks_forty_percent_preset_for_forty_five_percent_brightness() {
        assert_eq!(checked_tray_brightness_preset(55.0), 40);
    }

    #[test]
    fn keeps_ten_percent_preset_for_exact_ten_percent_brightness() {
        assert_eq!(checked_tray_brightness_preset(90.0), 10);
    }

    #[test]
    fn keeps_ten_percent_preset_for_five_percent_brightness() {
        assert_eq!(checked_tray_brightness_preset(95.0), 10);
    }

    #[test]
    fn floors_fractional_brightness_to_the_lower_preset_bucket() {
        assert_eq!(checked_tray_brightness_preset(9.1), 90);
        assert_eq!(checked_tray_brightness_preset(10.0), 90);
    }
}

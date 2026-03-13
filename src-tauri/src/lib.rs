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
    Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::{
    hotkeys::{HotkeyAction, HotkeyManager},
    state::{initialize_state, refresh_state, reset_dimming_before_exit, start_loop},
};

const SETTINGS_WINDOW_WIDTH: f64 = 896.0;
const SETTINGS_WINDOW_HEIGHT: f64 = 602.0;
const TRAY_ICON_WIDTH: u32 = 32;
const TRAY_ICON_HEIGHT: u32 = 32;
const TRAY_ICON_RGBA: &[u8] = include_bytes!("../icons/tray-icon.rgba");

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
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .visible(false)
    .build()?;
    configure_settings_window(&window);
    Ok(window)
}

pub(crate) fn open_settings_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = ensure_settings_window(app)?;
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
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => {
                        // Treat a tray click like "show me the settings window".
                        if let Err(error) = open_settings_window(tray.app_handle()) {
                            eprintln!("Failed to open settings window: {error}");
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

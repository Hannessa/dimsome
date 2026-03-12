mod commands;
mod hotkeys;
mod models;
mod schedule;
mod settings;
mod startup;
mod state;
mod windows;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, Position, Rect, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

use crate::{
    hotkeys::{HotkeyAction, HotkeyManager},
    state::{initialize_state, refresh_state, start_loop},
};

const QUICK_PANEL_WIDTH: f64 = 330.0;
const QUICK_PANEL_HEIGHT: f64 = 360.0;

fn configure_settings_window(window: &tauri::WebviewWindow) {
    let settings_window = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = settings_window.hide();
        }
    });
}

fn build_quick_panel(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window("quick-panel") {
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            "quick-panel",
            WebviewUrl::App("index.html?window=quick-panel".into()),
        )
        .title("Dimsome Quick Panel")
        .inner_size(QUICK_PANEL_WIDTH, QUICK_PANEL_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?
    };

    let quick_panel = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => {
            let _ = quick_panel.hide();
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = quick_panel.hide();
        }
        _ => {}
    });

    window.hide()?;
    Ok(())
}

fn resolve_anchor_rect(position: PhysicalPosition<f64>, rect: Rect) -> (i32, i32, i32, i32) {
    let rect_position = rect.position.to_physical::<f64>(1.0);
    let rect_size = rect.size.to_physical::<f64>(1.0);

    let width = rect_size.width.round().max(1.0) as i32;
    let height = rect_size.height.round().max(1.0) as i32;
    let left = if width > 1 {
        rect_position.x.round() as i32
    } else {
        (position.x - 12.0).round() as i32
    };
    let top = if height > 1 {
        rect_position.y.round() as i32
    } else {
        (position.y - 12.0).round() as i32
    };

    (left, top, width, height)
}

fn show_quick_panel(
    window: &tauri::WebviewWindow,
    anchor_left: i32,
    anchor_top: i32,
    anchor_width: i32,
    anchor_height: i32,
) {
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    let (x, y) = crate::windows::calculate_quick_panel_position(
        anchor_left,
        anchor_top,
        anchor_width,
        anchor_height,
        QUICK_PANEL_WIDTH.round() as i32,
        QUICK_PANEL_HEIGHT.round() as i32,
    );

    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn run() {
    tauri::Builder::default()
        .manage(initialize_state())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_effective_state,
            commands::apply_manual_dim,
            commands::pause_schedule,
            commands::resume_schedule,
            commands::get_startup_state,
            commands::set_startup_enabled,
            commands::show_settings_window,
            commands::exit_app
        ])
        .setup(|app| {
            if let Some(settings_window) = app.get_webview_window("settings") {
                configure_settings_window(&settings_window);
                settings_window.hide()?;
            }

            build_quick_panel(app.handle())?;

            let shared_for_hotkeys = app.state::<crate::state::SharedState>().inner().clone();
            let shared_for_hotkeys_handler = shared_for_hotkeys.clone();
            let app_handle_for_hotkeys = app.handle().clone();
            let hotkey_manager = HotkeyManager::new(move |action| {
                let shared = shared_for_hotkeys_handler.clone();
                let app_handle = app_handle_for_hotkeys.clone();
                tauri::async_runtime::spawn(async move {
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
            let initial_hotkeys = shared_for_hotkeys.blocking_read().settings.manual_hotkeys.clone();
            hotkey_manager.update_bindings(initial_hotkeys);
            app.manage(hotkey_manager);

            let open_settings = MenuItemBuilder::with_id("open_settings", "Open Settings").build(app)?;
            let pause_resume = MenuItemBuilder::with_id("pause_resume", "Pause / Resume").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Exit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&open_settings, &pause_resume, &quit])
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open_settings" => {
                        if let Some(quick_panel) = app.get_webview_window("quick-panel") {
                            let _ = quick_panel.hide();
                        }
                        if let Some(window) = app.get_webview_window("settings") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "pause_resume" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(shared) = app_handle.try_state::<crate::state::SharedState>() {
                                let current = shared.inner().read().await.current_state.clone();
                                if current.mode == crate::models::EffectiveDimMode::Paused {
                                    let _ = crate::state::resume(shared.inner(), &app_handle).await;
                                } else {
                                    let _ = crate::state::pause(shared.inner(), &app_handle).await;
                                }
                            }
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        rect,
                        ..
                    } => {
                        if let Some(window) = tray.app_handle().get_webview_window("quick-panel") {
                            let (left, top, width, height) = resolve_anchor_rect(position, rect);
                            show_quick_panel(&window, left, top, width, height);
                        }
                    }
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => {
                        if let Some(quick_panel) = tray.app_handle().get_webview_window("quick-panel") {
                            let _ = quick_panel.hide();
                        }
                        if let Some(window) = tray.app_handle().get_webview_window("settings") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            let shared = app.state::<crate::state::SharedState>().inner().clone();
            let shared_for_refresh = shared.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = refresh_state(&shared_for_refresh, Some(&app_handle)).await;
            });
            start_loop(shared, app.handle().clone());
            let _ = app.emit("startup_state_changed", crate::startup::get_startup_state());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Dimsome Tauri");
}



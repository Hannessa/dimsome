use tauri::{AppHandle, Emitter, Manager, State, Window};

use crate::{
    hotkeys::HotkeyManager,
    models::{AppSettings, EffectiveDimState, StartupRegistrationState},
    startup,
    state::{self, SharedState},
};

#[tauri::command]
pub async fn get_settings(state: State<'_, SharedState>) -> Result<AppSettings, String> {
    Ok(state.read().await.settings.clone())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, SharedState>,
    hotkeys: State<'_, HotkeyManager>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let saved = state::update_settings(&state.inner().clone(), &app, settings).await?;
    hotkeys.update_bindings(saved.manual_hotkeys.clone());
    Ok(saved)
}

#[tauri::command]
pub async fn get_effective_state(state: State<'_, SharedState>) -> Result<EffectiveDimState, String> {
    Ok(state.read().await.current_state.clone())
}

#[tauri::command]
pub async fn apply_manual_dim(
    app: AppHandle,
    state: State<'_, SharedState>,
    dim_percent: f64,
) -> Result<EffectiveDimState, String> {
    Ok(state::apply_manual_dim(&state.inner().clone(), &app, dim_percent).await)
}

#[tauri::command]
pub async fn pause_schedule(app: AppHandle, state: State<'_, SharedState>) -> Result<EffectiveDimState, String> {
    Ok(state::pause(&state.inner().clone(), &app).await)
}

#[tauri::command]
pub async fn resume_schedule(app: AppHandle, state: State<'_, SharedState>) -> Result<EffectiveDimState, String> {
    Ok(state::resume(&state.inner().clone(), &app).await)
}

#[tauri::command]
pub async fn get_startup_state() -> Result<StartupRegistrationState, String> {
    Ok(startup::get_startup_state())
}

#[tauri::command]
pub async fn set_startup_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<StartupRegistrationState, String> {
    let executable = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .display()
        .to_string();
    let state = startup::set_startup_enabled(enabled, &executable)?;
    let _ = app.emit("startup_state_changed", state.clone());
    Ok(state)
}

#[tauri::command]
pub async fn exit_app(window: Window) -> Result<(), String> {
    window.app_handle().exit(0);
    Ok(())
}

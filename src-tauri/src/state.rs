use std::{sync::Arc, time::Duration};

use chrono::DateTime;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::{
    models::{AppSettings, EffectiveDimMode, EffectiveDimState, ManualOverrideSession},
    schedule::{get_effective_dim, now_fixed, normalize_settings, resolve_state},
    settings,
    windows::DimmingManager,
};

pub struct RuntimeState {
    pub settings: AppSettings,
    pub current_state: EffectiveDimState,
    pub manual_dim_percent: Option<f64>,
    pub manual_override_until: Option<DateTime<chrono::FixedOffset>>,
    pub schedule_paused: bool,
    pub paused_dim_percent: f64,
    pub dimming_manager: DimmingManager,
}

impl RuntimeState {
    fn new(settings: AppSettings) -> Self {
        Self {
            settings,
            current_state: EffectiveDimState {
                mode: EffectiveDimMode::Auto,
                current_dim_percent: 0.0,
                manual_override_until: None,
            },
            manual_dim_percent: None,
            manual_override_until: None,
            schedule_paused: false,
            paused_dim_percent: 0.0,
            dimming_manager: DimmingManager::new(),
        }
    }
}

pub type SharedState = Arc<RwLock<RuntimeState>>;

pub fn initialize_state() -> SharedState {
    let settings = settings::load_settings();
    Arc::new(RwLock::new(RuntimeState::new(normalize_settings(settings))))
}

pub async fn refresh_state(shared: &SharedState, app: Option<&AppHandle>) -> EffectiveDimState {
    let mut state = shared.write().await;
    let now = now_fixed();
    let next = resolve_state(
        &state.settings,
        &ManualOverrideSession {
            is_paused: state.schedule_paused,
            manual_dim_percent: state.manual_dim_percent,
            manual_override_until: state.manual_override_until,
            paused_dim_percent: state.paused_dim_percent,
        },
        now,
    );

    state.current_state = next.clone();
    state
        .dimming_manager
        .sync(state.settings.dimming_method.clone(), next.current_dim_percent);

    if next.mode == EffectiveDimMode::Auto {
        state.manual_dim_percent = None;
        state.manual_override_until = None;
    }

    if let Some(app) = app {
        let _ = app.emit("state_changed", next.clone());
    }

    next
}

pub fn start_loop(shared: SharedState, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = refresh_state(&shared, Some(&app)).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

pub async fn update_settings(shared: &SharedState, app: &AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let saved = settings::save_settings(&settings)?;
    {
        let mut state = shared.write().await;
        state.settings = saved.clone();
        if let Ok(schedule) = get_effective_dim(&state.settings, now_fixed()) {
            state.manual_override_until = schedule.next_transition_start;
        }
    }
    let _ = refresh_state(shared, Some(app)).await;
    let _ = app.emit("settings_saved", saved.clone());
    Ok(saved)
}

pub async fn apply_manual_dim(shared: &SharedState, app: &AppHandle, dim_percent: f64) -> EffectiveDimState {
    {
        let mut state = shared.write().await;
        state.manual_dim_percent = Some(dim_percent);
        state.schedule_paused = false;
        state.manual_override_until = get_effective_dim(&state.settings, now_fixed())
            .ok()
            .and_then(|schedule| schedule.next_transition_start);
    }
    refresh_state(shared, Some(app)).await
}

pub async fn nudge(shared: &SharedState, app: &AppHandle, direction: f64) -> EffectiveDimState {
    let next_dim_percent = {
        let state = shared.read().await;
        state.current_state.current_dim_percent + (state.settings.dim_step_percent * direction)
    };

    apply_manual_dim(shared, app, next_dim_percent).await
}

pub async fn pause(shared: &SharedState, app: &AppHandle) -> EffectiveDimState {
    {
        let mut state = shared.write().await;
        state.schedule_paused = true;
        state.manual_dim_percent = None;
        state.manual_override_until = None;
        state.paused_dim_percent = state.current_state.current_dim_percent;
    }
    refresh_state(shared, Some(app)).await
}

pub async fn resume(shared: &SharedState, app: &AppHandle) -> EffectiveDimState {
    {
        let mut state = shared.write().await;
        state.schedule_paused = false;
        state.manual_dim_percent = None;
        state.manual_override_until = None;
    }
    refresh_state(shared, Some(app)).await
}

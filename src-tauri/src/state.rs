use std::{sync::Arc, time::Duration};

use chrono::DateTime;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::{
    models::{
        AppSettings, DimmingCapabilities, DimmingMethod, EffectiveDimMode, EffectiveDimState,
        ManualOverrideSession,
    },
    schedule::{
        clamp_dim_precise, get_effective_dim, normalize_settings, now_fixed, resolve_state,
    },
    settings,
    windows::{probe_dimming_capabilities, DimmingManager},
};

pub struct RuntimeState {
    pub settings: AppSettings,
    pub dimming_capabilities: DimmingCapabilities,
    pub current_state: EffectiveDimState,
    pub manual_dim_percent: Option<f64>,
    pub manual_override_until: Option<DateTime<chrono::FixedOffset>>,
    pub schedule_paused: bool,
    pub paused_dim_percent: f64,
    pub dimming_manager: DimmingManager,
}

impl RuntimeState {
    fn new(settings: AppSettings) -> Self {
        // Probe the platform once so unsupported dimming modes can be filtered immediately.
        let dimming_capabilities = probe_dimming_capabilities();
        let settings =
            coerce_settings_for_capabilities(normalize_settings(settings), &dimming_capabilities);

        Self {
            settings,
            dimming_capabilities,
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
    Arc::new(RwLock::new(RuntimeState::new(settings)))
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

    // Keep the in-memory state and the active dimming engine synchronized.
    state.current_state = next.clone();
    state.dimming_manager.sync(
        state.settings.dimming_method.clone(),
        next.current_dim_percent,
    );

    if next.mode == EffectiveDimMode::Auto {
        state.manual_dim_percent = None;
        state.manual_override_until = None;
    }

    if let Some(app) = app {
        let _ = app.emit("state_changed", next.clone());
    }

    next
}

pub async fn reset_dimming_before_exit(shared: &SharedState) {
    let state = shared.read().await;
    state
        .dimming_manager
        .reset_to_full_brightness(state.settings.dimming_method.clone());
}

pub fn start_loop(shared: SharedState, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            // Re-evaluate on a short interval so schedule transitions stay smooth.
            let _ = refresh_state(&shared, Some(&app)).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

pub async fn update_settings(
    shared: &SharedState,
    app: &AppHandle,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let capabilities = shared.read().await.dimming_capabilities.clone();
    let saved =
        settings::save_settings(&coerce_settings_for_capabilities(settings, &capabilities))?;
    {
        let mut state = shared.write().await;
        state.settings = saved.clone();
        if let Ok(schedule) = get_effective_dim(&state.settings, now_fixed()) {
            // Expire any manual override when the next scheduled transition begins.
            state.manual_override_until = schedule.next_transition_start;
        }
    }
    let _ = refresh_state(shared, Some(app)).await;
    let _ = app.emit("settings_saved", saved.clone());
    Ok(saved)
}

pub async fn apply_manual_dim(
    shared: &SharedState,
    app: &AppHandle,
    dim_percent: f64,
) -> EffectiveDimState {
    {
        let mut state = shared.write().await;
        // Manual changes resume scheduling later, rather than permanently disabling it.
        state.manual_dim_percent = Some(dim_percent);
        state.schedule_paused = false;
        state.manual_override_until = get_effective_dim(&state.settings, now_fixed())
            .ok()
            .and_then(|schedule| schedule.next_transition_start);
    }
    refresh_state(shared, Some(app)).await
}

fn nudge_target(
    current_dim_percent: f64,
    step_percent: f64,
    direction: f64,
    maximum_dim_percent: f64,
) -> f64 {
    // Apply the requested step first so both hotkeys and future nudges share one calculation path.
    let nudged_dim_percent = current_dim_percent + (step_percent * direction);

    // Cap hotkey dimming so the remaining screen brightness never drops below five percent.
    if direction > 0.0 {
        return clamp_dim_precise(nudged_dim_percent.min(maximum_dim_percent));
    }

    clamp_dim_precise(nudged_dim_percent)
}

pub async fn nudge_hotkey(
    shared: &SharedState,
    app: &AppHandle,
    direction: f64,
) -> EffectiveDimState {
    let next_dim_percent = {
        let state = shared.read().await;

        // Keep hotkey-driven dimming at or below ninety-five percent so brightness stays at least five percent.
        nudge_target(
            state.current_state.current_dim_percent,
            state.settings.dim_step_percent,
            direction,
            95.0,
        )
    };

    apply_manual_dim(shared, app, next_dim_percent).await
}

pub async fn pause(shared: &SharedState, app: &AppHandle) -> EffectiveDimState {
    {
        let mut state = shared.write().await;
        // Freeze the current dim value so pause keeps the desktop exactly as-is.
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

fn coerce_settings_for_capabilities(
    mut settings: AppSettings,
    capabilities: &DimmingCapabilities,
) -> AppSettings {
    if !capabilities.magnification_available
        && settings.dimming_method == DimmingMethod::Magnification
    {
        // Downgrade unsupported persisted settings to the safest available method.
        settings.dimming_method = DimmingMethod::Overlay;
    }

    settings
}

#[cfg(test)]
mod tests {
    use super::{coerce_settings_for_capabilities, nudge_target};
    use crate::models::{AppSettings, DimmingCapabilities, DimmingMethod};

    #[test]
    fn hotkey_maximum_stops_dim_more_at_ninety_five_percent() {
        assert_eq!(nudge_target(93.0, 5.0, 1.0, 95.0), 95.0);
    }

    #[test]
    fn hotkey_cap_does_not_change_dim_less_nudges() {
        assert_eq!(nudge_target(8.0, 5.0, -1.0, 95.0), 3.0);
    }

    #[test]
    fn high_cap_does_not_change_regular_dim_more_steps_below_limit() {
        assert_eq!(nudge_target(40.0, 5.0, 1.0, 95.0), 45.0);
    }

    #[test]
    fn unsupported_magnification_defaults_fall_back_to_overlay() {
        let settings = AppSettings::default();
        let capabilities = DimmingCapabilities {
            // Simulate a machine or runtime where Magnification cannot be enabled.
            magnification_available: false,
            magnification_status_text: "Unavailable".into(),
        };

        let coerced = coerce_settings_for_capabilities(settings, &capabilities);

        // Keep the first-run settings usable even when the preferred engine is unavailable.
        assert_eq!(coerced.dimming_method, DimmingMethod::Overlay);
    }
}

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_VERSION: i32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AppearanceMode {
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DimmingMethod {
    Overlay,
    Gamma,
    Magnification,
}

pub fn default_dimming_method() -> DimmingMethod {
    DimmingMethod::Magnification
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub version: i32,
    pub startup_enabled: bool,
    pub schedule_enabled: bool,
    pub dim_step_percent: f64,
    #[serde(default = "default_dimming_method")]
    pub dimming_method: DimmingMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance_mode: Option<AppearanceMode>,
    pub manual_hotkeys: ManualHotkeys,
    pub schedule_points: Vec<SchedulePoint>,
}

impl Default for AppSettings {
    fn default() -> Self {
        // Seed first-run installs with the preferred startup, dimming, and schedule defaults.
        Self {
            version: CURRENT_VERSION,
            startup_enabled: false,
            schedule_enabled: true,
            dim_step_percent: 5.0,
            dimming_method: default_dimming_method(),
            appearance_mode: None,
            manual_hotkeys: ManualHotkeys::default(),
            schedule_points: vec![
                // Restore full brightness by 08:00 using the new morning fade duration.
                SchedulePoint {
                    id: Uuid::new_v4(),
                    time_of_day: "08:00:00".to_string(),
                    target_dim_percent: 0.0,
                    transition_minutes: 60,
                    enabled: true,
                },
                // Reach the nighttime dim target at 23:00 with a longer evening fade.
                SchedulePoint {
                    id: Uuid::new_v4(),
                    time_of_day: "23:00:00".to_string(),
                    target_dim_percent: 50.0,
                    transition_minutes: 120,
                    enabled: true,
                },
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManualHotkeys {
    pub dim_more: HotkeyBinding,
    pub dim_less: HotkeyBinding,
}

impl Default for ManualHotkeys {
    fn default() -> Self {
        // Default to Alt+PageUp/PageDown so the shortcuts work on a fresh install.
        Self {
            dim_more: HotkeyBinding {
                enabled: true,
                modifiers: "Alt".to_string(),
                key: "PageDown".to_string(),
            },
            dim_less: HotkeyBinding {
                enabled: true,
                modifiers: "Alt".to_string(),
                key: "PageUp".to_string(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyBinding {
    pub enabled: bool,
    pub modifiers: String,
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePoint {
    pub id: Uuid,
    pub time_of_day: String,
    pub target_dim_percent: f64,
    pub transition_minutes: i32,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum EffectiveDimMode {
    Auto,
    Manual,
    Paused,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveDimState {
    pub mode: EffectiveDimMode,
    pub current_dim_percent: f64,
    pub manual_override_until: Option<DateTime<FixedOffset>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManualOverrideSession {
    pub is_paused: bool,
    pub manual_dim_percent: Option<f64>,
    pub manual_override_until: Option<DateTime<FixedOffset>>,
    pub paused_dim_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEvaluation {
    pub current_dim_percent: f64,
    pub next_transition_start: Option<DateTime<FixedOffset>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupRegistrationState {
    pub is_enabled: bool,
    pub can_change: bool,
    pub status_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DimmingCapabilities {
    pub magnification_available: bool,
    pub magnification_status_text: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn app_settings_default_to_magnification_when_dimming_method_is_missing() {
        let json = json!({
            "version": 1,
            "startupEnabled": true,
            "scheduleEnabled": true,
            "dimStepPercent": 5.0,
            "manualHotkeys": {
                "dimMore": {
                    "enabled": true,
                    "modifiers": "Alt",
                    "key": "PageDown"
                },
                "dimLess": {
                    "enabled": true,
                    "modifiers": "Alt",
                    "key": "PageUp"
                }
            },
            "schedulePoints": []
        });

        // Legacy settings files should still deserialize after new fields are added.
        let settings: AppSettings =
            serde_json::from_value(json).expect("legacy settings should deserialize");

        assert_eq!(settings.dimming_method, DimmingMethod::Magnification);
    }

    #[test]
    fn app_settings_default_match_first_run_preferences() {
        let settings = AppSettings::default();

        // Confirm the fresh-install settings match the intended first-run experience.
        assert_eq!(settings.startup_enabled, false);
        assert_eq!(settings.schedule_enabled, true);
        assert_eq!(settings.dim_step_percent, 5.0);
        assert_eq!(settings.dimming_method, DimmingMethod::Magnification);
        assert_eq!(settings.appearance_mode, None);
        assert_eq!(settings.manual_hotkeys, ManualHotkeys::default());
        assert_eq!(settings.schedule_points.len(), 2);

        // Check the morning point lands at full brightness at 08:00.
        assert_eq!(settings.schedule_points[0].time_of_day, "08:00:00");
        assert_eq!(settings.schedule_points[0].target_dim_percent, 0.0);
        assert_eq!(settings.schedule_points[0].transition_minutes, 60);
        assert_eq!(settings.schedule_points[0].enabled, true);

        // Check the evening point lands at fifty percent dim at 23:00.
        assert_eq!(settings.schedule_points[1].time_of_day, "23:00:00");
        assert_eq!(settings.schedule_points[1].target_dim_percent, 50.0);
        assert_eq!(settings.schedule_points[1].transition_minutes, 120);
        assert_eq!(settings.schedule_points[1].enabled, true);
    }

    #[test]
    fn dimming_method_supports_magnification_value() {
        let json = json!("magnification");
        let method: DimmingMethod =
            serde_json::from_value(json).expect("magnification should deserialize");

        assert_eq!(method, DimmingMethod::Magnification);
    }
}
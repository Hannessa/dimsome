use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_VERSION: i32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AppearanceMode {
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub version: i32,
    pub startup_enabled: bool,
    pub schedule_enabled: bool,
    pub dim_step_percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance_mode: Option<AppearanceMode>,
    pub manual_hotkeys: ManualHotkeys,
    pub schedule_points: Vec<SchedulePoint>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            startup_enabled: true,
            schedule_enabled: true,
            dim_step_percent: 5.0,
            appearance_mode: None,
            manual_hotkeys: ManualHotkeys::default(),
            schedule_points: vec![
                SchedulePoint {
                    id: Uuid::new_v4(),
                    time_of_day: "07:00:00".to_string(),
                    target_dim_percent: 0.0,
                    transition_minutes: 30,
                    enabled: true,
                },
                SchedulePoint {
                    id: Uuid::new_v4(),
                    time_of_day: "23:00:00".to_string(),
                    target_dim_percent: 50.0,
                    transition_minutes: 60,
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

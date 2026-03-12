use std::{fs, path::PathBuf};

use crate::{models::AppSettings, schedule::normalize_settings};

pub fn settings_path() -> PathBuf {
    let mut base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.push("Dimsome");
    base.push("settings.json");
    base
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
            Ok(parsed) => normalize_settings(parsed),
            Err(_) => normalize_settings(AppSettings::default()),
        },
        Err(_) => normalize_settings(AppSettings::default()),
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<AppSettings, String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let normalized = normalize_settings(settings.clone());
    let json = serde_json::to_string_pretty(&normalized).map_err(|error| error.to_string())?;
    fs::write(&path, json).map_err(|error| error.to_string())?;
    Ok(normalized)
}

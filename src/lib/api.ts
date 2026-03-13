import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, EffectiveDimState, StartupRegistrationState } from "../types/app";

export function getSettings() {
  return invoke<AppSettings>("get_settings");
}

export function saveSettings(settings: AppSettings) {
  return invoke<AppSettings>("save_settings", { settings });
}

export function getEffectiveState() {
  return invoke<EffectiveDimState>("get_effective_state");
}

export function applyManualDim(dimPercent: number) {
  return invoke<EffectiveDimState>("apply_manual_dim", { dimPercent });
}

export function pauseSchedule() {
  return invoke<EffectiveDimState>("pause_schedule");
}

export function resumeSchedule() {
  return invoke<EffectiveDimState>("resume_schedule");
}

export function getStartupState() {
  return invoke<StartupRegistrationState>("get_startup_state");
}

export function setStartupEnabled(enabled: boolean) {
  return invoke<StartupRegistrationState>("set_startup_enabled", { enabled });
}

export function exitApp() {
  return invoke<void>("exit_app");
}

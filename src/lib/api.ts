import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  DimmingCapabilities,
  EffectiveDimState,
  StartupRegistrationState
} from "../types/app";

// Keep all Tauri command names in one place so the UI calls stay consistent.
export function getSettings() {
  return invoke<AppSettings>("get_settings");
}

export function getDimmingCapabilities() {
  return invoke<DimmingCapabilities>("get_dimming_capabilities");
}

export function saveSettings(settings: AppSettings) {
  return invoke<AppSettings>("save_settings", { settings });
}

export function getEffectiveState() {
  return invoke<EffectiveDimState>("get_effective_state");
}

export function applyManualDimAndDisableSchedule(dimPercent: number) {
  return invoke<EffectiveDimState>("apply_manual_dim_and_disable_schedule", { dimPercent });
}

export function enableSchedule() {
  return invoke<EffectiveDimState>("enable_schedule");
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

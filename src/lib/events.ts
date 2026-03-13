import { listen } from "@tauri-apps/api/event";
import type { AppSettings, EffectiveDimState, StartupRegistrationState } from "../types/app";

// Wrap the app-wide Tauri events in small typed helpers for the settings view.
export const onStateChanged = (handler: (state: EffectiveDimState) => void) =>
  listen<EffectiveDimState>("state_changed", (event) => handler(event.payload));

export const onSettingsSaved = (handler: (settings: AppSettings) => void) =>
  listen<AppSettings>("settings_saved", (event) => handler(event.payload));

export const onStartupStateChanged = (handler: (state: StartupRegistrationState) => void) =>
  listen<StartupRegistrationState>("startup_state_changed", (event) => handler(event.payload));
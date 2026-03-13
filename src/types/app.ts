// Share the app's settings and runtime contracts between Vue and Tauri.
export interface HotkeyBinding {
  enabled: boolean;
  modifiers: string;
  key: string;
}

export interface ManualHotkeys {
  dimMore: HotkeyBinding;
  dimLess: HotkeyBinding;
}

export interface SchedulePoint {
  id: string;
  timeOfDay: string;
  targetDimPercent: number;
  transitionMinutes: number;
  enabled: boolean;
}

export type AppearanceMode = "light" | "dark";
export type DimmingMethod = "overlay" | "gamma" | "magnification";

export interface AppSettings {
  version: number;
  startupEnabled: boolean;
  scheduleEnabled: boolean;
  dimStepPercent: number;
  dimmingMethod: DimmingMethod;
  appearanceMode?: AppearanceMode;
  manualHotkeys: ManualHotkeys;
  schedulePoints: SchedulePoint[];
}

export interface DimmingCapabilities {
  magnificationAvailable: boolean;
  magnificationStatusText: string;
}

export type EffectiveDimMode = "Auto" | "Manual" | "Paused";

export interface EffectiveDimState {
  mode: EffectiveDimMode;
  currentDimPercent: number;
  manualOverrideUntil: string | null;
}

export interface StartupRegistrationState {
  isEnabled: boolean;
  canChange: boolean;
  statusText: string;
}
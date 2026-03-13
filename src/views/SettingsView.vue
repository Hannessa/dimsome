<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import Button from "primevue/button";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import Slider from "primevue/slider";
import ToggleSwitch from "primevue/toggleswitch";
import {
  getEffectiveState,
  getSettings,
  getStartupState,
  pauseSchedule,
  resumeSchedule,
  saveSettings,
  setStartupEnabled
} from "../lib/api";
import { onSettingsSaved, onStartupStateChanged, onStateChanged } from "../lib/events";
import { syncAppearanceMode } from "../lib/theme";
import type { AppSettings, AppearanceMode, EffectiveDimState, StartupRegistrationState } from "../types/app";

const settings = ref<AppSettings | null>(null);
const currentState = ref<EffectiveDimState | null>(null);
const startupState = ref<StartupRegistrationState | null>(null);
const statusMessage = ref("");
const appearanceModeOptions: Array<{ label: string; value: "system" | AppearanceMode }> = [
  { label: "Follow system", value: "system" },
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" }
];

const dimSummary = computed(() => `${settings.value?.dimStepPercent ?? 0}% per hotkey press`);
const selectedAppearanceMode = computed({
  get: () => settings.value?.appearanceMode ?? "system",
  set: (value: "system" | AppearanceMode) => {
    if (!settings.value) {
      return;
    }

    settings.value.appearanceMode = value === "system" ? undefined : value;
    syncAppearanceMode(settings.value.appearanceMode);
  }
});
const currentModeText = computed(() => {
  const mode = currentState.value?.mode ?? "Auto";
  if (mode === "Manual") return "Manual override";
  if (mode === "Paused") return "Schedule paused";
  return "Following schedule";
});

function ensureSettings() {
  if (!settings.value) {
    throw new Error("Settings are not loaded.");
  }

  return settings.value;
}

function addSchedulePoint() {
  const model = ensureSettings();
  const last = [...model.schedulePoints].sort((a, b) => a.timeOfDay.localeCompare(b.timeOfDay)).at(-1);
  const nextHour = last ? (Number.parseInt(last.timeOfDay.slice(0, 2), 10) + 1) % 24 : 23;
  model.schedulePoints.push({
    id: crypto.randomUUID(),
    timeOfDay: `${nextHour.toString().padStart(2, "0")}:00:00`,
    targetDimPercent: 30,
    transitionMinutes: 30,
    enabled: true
  });
}

function removeSchedulePoint(id: string) {
  const model = ensureSettings();
  model.schedulePoints = model.schedulePoints.filter((point) => point.id !== id);
}

async function save() {
  const model = ensureSettings();
  statusMessage.value = "";

  const startup = await setStartupEnabled(model.startupEnabled);
  startupState.value = startup;
  model.startupEnabled = startup.isEnabled;

  settings.value = await saveSettings(model);
  syncAppearanceMode(settings.value.appearanceMode);
  statusMessage.value = "Settings saved.";
}

async function initialize() {
  const [loadedSettings, loadedState, loadedStartupState] = await Promise.all([
    getSettings(),
    getEffectiveState(),
    getStartupState()
  ]);

  settings.value = loadedSettings;
  currentState.value = loadedState;
  startupState.value = loadedStartupState;
  syncAppearanceMode(loadedSettings.appearanceMode);
}

onMounted(async () => {
  await initialize();
});

onStateChanged((payload) => {
  currentState.value = payload;
});

onSettingsSaved((payload) => {
  settings.value = payload;
  syncAppearanceMode(payload.appearanceMode);
});

onStartupStateChanged((payload) => {
  startupState.value = payload;
});
</script>

<template>
  <main class="page page-settings" v-if="settings">
    <section class="hero">
      <div>
        <p class="eyebrow">Dimsome</p>
        <h1>Software dimming for deeper nights and smoother fades.</h1>
      </div>
      <div class="state-card">
        <div class="section-label">Current state</div>
        <div>{{ currentModeText }}</div>
        <div class="state-value">{{ Math.round(currentState?.currentDimPercent ?? 0) }}% dim</div>
      </div>
    </section>

    <section class="grid">
      <div class="card schedule-card">
        <div class="section-label">Schedule</div>
        <p class="muted">Each point defines the target dim level and how many minutes the ramp should take before it lands.</p>

        <div class="schedule-list">
          <div class="schedule-item" v-for="point in settings.schedulePoints" :key="point.id">
            <label class="field checkbox-field">
              <span>Enabled</span>
              <ToggleSwitch v-model="point.enabled" />
            </label>
            <label class="field">
              <span>Time</span>
              <input type="time" step="60" v-model="point.timeOfDay" />
            </label>
            <label class="field">
              <span>Dim %</span>
              <InputNumber v-model="point.targetDimPercent" :min="0" :max="95" fluid />
            </label>
            <label class="field">
              <span>Fade min</span>
              <InputNumber v-model="point.transitionMinutes" :min="0" :max="1439" fluid />
            </label>
            <Button label="Remove" severity="secondary" variant="outlined" @click="removeSchedulePoint(point.id)" />
          </div>
        </div>

        <Button label="Add Schedule Point" severity="secondary" variant="outlined" @click="addSchedulePoint" />
      </div>

      <div class="side-column">
        <div class="card">
          <div class="section-label">Appearance</div>
          <label class="field">
            <span>Color scheme</span>
            <Select
              v-model="selectedAppearanceMode"
              :options="appearanceModeOptions"
              option-label="label"
              option-value="value"
              fluid
            />
          </label>
          <p class="muted">If you leave this on Follow system, PrimeVue tracks the operating system color scheme.</p>
        </div>

        <div class="card">
          <div class="section-label">Automation</div>
          <label class="field checkbox-field">
            <span>Enable automatic schedule</span>
            <ToggleSwitch v-model="settings.scheduleEnabled" />
          </label>
          <label class="field checkbox-field">
            <span>Launch at sign-in</span>
            <ToggleSwitch
              v-model="settings.startupEnabled"
              :disabled="startupState ? !startupState.canChange : false"
            />
          </label>
          <p class="muted">{{ startupState?.statusText ?? "Loading startup state..." }}</p>
          <label class="field">
            <span>Hotkey step size</span>
            <Slider v-model="settings.dimStepPercent" :min="1" :max="25" :step="1" />
          </label>
          <p class="muted">{{ dimSummary }}</p>
        </div>

        <div class="card">
          <div class="section-label">Hotkeys</div>
          <label class="field">
            <span>Dim more key</span>
            <InputText v-model="settings.manualHotkeys.dimMore.key" fluid />
          </label>
          <label class="field">
            <span>Dim less key</span>
            <InputText v-model="settings.manualHotkeys.dimLess.key" fluid />
          </label>
          <p class="muted">Modifier handling is preserved in the backend JSON contract; this first pass exposes the key names directly.</p>
        </div>

        <div class="card">
          <div class="section-label">Actions</div>
          <div class="action-row action-row-buttons">
            <Button label="Save Settings" @click="save" />
            <Button label="Pause Schedule" severity="secondary" variant="outlined" @click="pauseSchedule" />
            <Button label="Resume Schedule" severity="secondary" variant="outlined" @click="resumeSchedule" />
          </div>
          <p class="status">{{ statusMessage }}</p>
        </div>
      </div>
    </section>
  </main>
</template>

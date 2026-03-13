<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import Button from "primevue/button";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import SelectButton from "primevue/selectbutton";
import Slider from "primevue/slider";
import ToggleSwitch from "primevue/toggleswitch";
import {
  applyManualDim,
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
const sliderBrightness = ref(100);
const selectedPanel = ref<"schedule" | "settings">("schedule");
const appearanceModeOptions: Array<{ label: string; value: "system" | AppearanceMode }> = [
  { label: "Follow system", value: "system" },
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" }
];
const panelOptions: Array<{ label: string; value: "schedule" | "settings" }> = [
  { label: "Schedule", value: "schedule" },
  { label: "Settings", value: "settings" }
];

const brightnessStepSummary = computed(() => `${settings.value?.dimStepPercent ?? 0}% per hotkey press`);
const currentBrightnessPercent = computed(() => 100 - Math.round(currentState.value?.currentDimPercent ?? 0));
const isFollowingSchedule = computed(() => (currentState.value?.mode ?? "Auto") === "Auto");
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

function toBrightnessPercent(dimPercent: number) {
  return 100 - Math.round(dimPercent);
}

function toDimPercent(brightnessPercent: number) {
  return Math.min(95, Math.max(0, 100 - brightnessPercent));
}

function syncSliderToState(state: EffectiveDimState | null) {
  sliderBrightness.value = 100 - Math.round(state?.currentDimPercent ?? 0);
}

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

async function applyBrightnessFromSlider(event: { value: number | number[] }) {
  const nextBrightness = Array.isArray(event.value) ? event.value[0] : event.value;
  sliderBrightness.value = nextBrightness;
  currentState.value = await applyManualDim(toDimPercent(nextBrightness));
  syncSliderToState(currentState.value);
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
  syncSliderToState(loadedState);
}

onMounted(async () => {
  await initialize();
});

onStateChanged((payload) => {
  currentState.value = payload;
  syncSliderToState(payload);
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
    <section class="hero hero-centered">
      <p class="eyebrow">Dimsome</p>
      <div class="hero-brightness">{{ currentBrightnessPercent }}% brightness</div>
      <div class="hero-slider-wrap">
        <Slider
          v-model="sliderBrightness"
          class="hero-slider"
          :min="5"
          :max="100"
          :step="1"
          @slideend="applyBrightnessFromSlider"
        />
      </div>
      <div class="hero-status-row">
        <span>{{ isFollowingSchedule ? "Following schedule" : "Manual override" }}</span>
        <Button
          v-if="!isFollowingSchedule"
          label="▶"
          text
          rounded
          aria-label="Resume schedule"
          @click="resumeSchedule"
        />
      </div>
    </section>

    <section class="panel-switcher">
      <SelectButton
        v-model="selectedPanel"
        :options="panelOptions"
        option-label="label"
        option-value="value"
        :allow-empty="false"
      />
    </section>

    <section v-if="selectedPanel === 'schedule'" class="panel-content panel-content-schedule">
      <div class="card schedule-card centered-card">
        <div class="section-label">Schedule</div>
        <p class="muted">Each point defines the target brightness level and how many minutes the ramp should take before it lands.</p>

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
              <span>Brightness %</span>
              <InputNumber
                :model-value="toBrightnessPercent(point.targetDimPercent)"
                @update:model-value="point.targetDimPercent = toDimPercent($event ?? 100)"
                :min="5"
                :max="100"
                fluid
              />
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
    </section>

    <section v-else class="panel-content panel-content-settings">
      <div class="settings-stack">
        <div class="card centered-card settings-card">
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

        <div class="card centered-card settings-card">
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
            <span>Brightness step size</span>
            <Slider v-model="settings.dimStepPercent" :min="1" :max="25" :step="1" />
          </label>
          <p class="muted">{{ brightnessStepSummary }}</p>
        </div>

        <div class="card centered-card settings-card">
          <div class="section-label">Hotkeys</div>
          <label class="field">
            <span>Decrease brightness key</span>
            <InputText v-model="settings.manualHotkeys.dimMore.key" fluid />
          </label>
          <label class="field">
            <span>Increase brightness key</span>
            <InputText v-model="settings.manualHotkeys.dimLess.key" fluid />
          </label>
          <p class="muted">Modifier handling is preserved in the backend JSON contract; this first pass exposes the key names directly.</p>
        </div>

        <div class="card centered-card settings-card">
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

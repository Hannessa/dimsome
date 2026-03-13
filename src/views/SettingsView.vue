<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import Button from "primevue/button";
import DatePicker from "primevue/datepicker";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import SelectButton from "primevue/selectbutton";
import AppSlider from "../components/AppSlider.vue";
import ToggleSwitch from "primevue/toggleswitch";
import {
  applyManualDim,
  getDimmingCapabilities,
  getEffectiveState,
  getSettings,
  getStartupState,
  resumeSchedule,
  saveSettings,
  setStartupEnabled
} from "../lib/api";
import { onSettingsSaved, onStartupStateChanged, onStateChanged } from "../lib/events";
import { syncAppearanceMode } from "../lib/theme";
import type {
  AppSettings,
  AppearanceMode,
  DimmingCapabilities,
  DimmingMethod,
  EffectiveDimState,
  StartupRegistrationState
} from "../types/app";

const settings = ref<AppSettings | null>(null);
const dimmingCapabilities = ref<DimmingCapabilities | null>(null);
const currentState = ref<EffectiveDimState | null>(null);
const startupState = ref<StartupRegistrationState | null>(null);
const sliderBrightness = ref(100);
const selectedPanel = ref<"schedule" | "settings">("schedule");
const isApplyingSliderBrightness = ref(false);
const pendingSliderBrightness = ref<number | null>(null);
const saveQueued = ref(false);
const isSaving = ref(false);
const skipAutosave = ref(true);
const lastSavedSnapshot = ref<string | null>(null);
const appearanceModeOptions: Array<{ label: string; value: "system" | AppearanceMode }> = [
  { label: "Follow system", value: "system" },
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" }
];
const panelOptions: Array<{ label: string; value: "schedule" | "settings" }> = [
  { label: "Schedule", value: "schedule" },
  { label: "Settings", value: "settings" }
];

const cardClass = "glass-card rounded-[24px] p-5";
const sectionLabelClass = "text-[0.9rem] uppercase tracking-[0.04em] text-[var(--muted)]";
const fieldClass = "grid gap-1.5";
const fieldLabelClass = "text-[0.9rem] uppercase tracking-[0.04em] text-[var(--muted)]";

const dimmingMethodOptions = computed<Array<{ label: string; value: DimmingMethod; disabled?: boolean }>>(() => [
  { label: "Black overlay", value: "overlay" },
  { label: "Gamma / LUT (experimental)", value: "gamma" },
  {
    label: "Magnification (experimental)",
    value: "magnification",
    disabled: !(dimmingCapabilities.value?.magnificationAvailable ?? false)
  }
]);
const dimmingMethodSummary = computed(() => {
  const gammaText = "Gamma / LUT may interact with Night Light, HDR, or display calibration.";
  const magnificationText = dimmingCapabilities.value?.magnificationStatusText ?? "Checking Magnification support...";
  return `${gammaText} ${magnificationText}`;
});
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

// Disable right-click
document.addEventListener("contextmenu", (event) => event.preventDefault());

function toBrightnessPercent(dimPercent: number) {
  return 100 - dimPercent;
}

function toDimPercent(brightnessPercent: number) {
  return Math.min(99, Math.max(0, 100 - brightnessPercent));
}

function syncSliderToState(state: EffectiveDimState | null) {
  sliderBrightness.value = 100 - (state?.currentDimPercent ?? 0);
}

function ensureSettings() {
  if (!settings.value) {
    throw new Error("Settings are not loaded.");
  }

  return settings.value;
}

function cloneSettings(model: AppSettings): AppSettings {
  return JSON.parse(JSON.stringify(model)) as AppSettings;
}

function serializeSettings(model: AppSettings) {
  return JSON.stringify(model);
}

function serializeAutosaveSettings(model: AppSettings) {
  return JSON.stringify({
    startupEnabled: model.startupEnabled,
    scheduleEnabled: model.scheduleEnabled,
    dimStepPercent: model.dimStepPercent,
    dimmingMethod: model.dimmingMethod,
    appearanceMode: model.appearanceMode ?? null,
    schedulePoints: model.schedulePoints
  });
}

function queueAutosave() {
  if (skipAutosave.value || !settings.value) {
    return;
  }

  saveQueued.value = true;
  void flushAutosaveQueue();
}

async function flushAutosaveQueue() {
  if (isSaving.value || !settings.value) {
    return;
  }

  isSaving.value = true;

  try {
    while (saveQueued.value && settings.value) {
      saveQueued.value = false;
      const snapshot = cloneSettings(settings.value);

      const startup = await setStartupEnabled(snapshot.startupEnabled);
      startupState.value = startup;
      snapshot.startupEnabled = startup.isEnabled;

      const saved = await saveSettings(snapshot);
      lastSavedSnapshot.value = serializeSettings(saved);

      skipAutosave.value = true;
      settings.value = saved;
      syncAppearanceMode(saved.appearanceMode);
      skipAutosave.value = false;

      if (settings.value && serializeSettings(settings.value) !== lastSavedSnapshot.value) {
        saveQueued.value = true;
      }
    }
  } catch (error) {
    console.error("Failed to auto-save settings.", error);
  } finally {
    isSaving.value = false;
  }
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

function scheduleTimeToDate(timeOfDay: string) {
  const [hour = "00", minute = "00", second = "00"] = timeOfDay.split(":");
  const value = new Date();
  value.setHours(Number.parseInt(hour, 10), Number.parseInt(minute, 10), Number.parseInt(second, 10), 0);
  return value;
}

function updateScheduleTime(point: AppSettings["schedulePoints"][number], value: Date | Date[] | (Date | null)[] | undefined | null) {
  if (!(value instanceof Date)) {
    return;
  }

  const hour = value.getHours().toString().padStart(2, "0");
  const minute = value.getMinutes().toString().padStart(2, "0");
  point.timeOfDay = `${hour}:${minute}:00`;
}

function saveHotkeys() {
  queueAutosave();
}

async function applyBrightnessFromSlider(event: { value: number | number[] }) {
  const nextBrightness = Array.isArray(event.value) ? event.value[0] : event.value;
  await applyBrightnessWhileDragging(nextBrightness);
}

async function applyBrightnessWhileDragging(nextBrightness: number) {
  sliderBrightness.value = nextBrightness;
  pendingSliderBrightness.value = nextBrightness;

  if (isApplyingSliderBrightness.value) {
    return;
  }

  isApplyingSliderBrightness.value = true;

  try {
    while (pendingSliderBrightness.value !== null) {
      const brightnessToApply = pendingSliderBrightness.value;
      pendingSliderBrightness.value = null;
      currentState.value = await applyManualDim(toDimPercent(brightnessToApply));
      syncSliderToState(currentState.value);
    }
  } finally {
    isApplyingSliderBrightness.value = false;
  }
}

async function initialize() {
  const [loadedSettings, loadedCapabilities, loadedState, loadedStartupState] = await Promise.all([
    getSettings(),
    getDimmingCapabilities(),
    getEffectiveState(),
    getStartupState()
  ]);

  settings.value = loadedSettings;
  dimmingCapabilities.value = loadedCapabilities;
  currentState.value = loadedState;
  startupState.value = loadedStartupState;
  lastSavedSnapshot.value = serializeSettings(loadedSettings);
  syncAppearanceMode(loadedSettings.appearanceMode);
  syncSliderToState(loadedState);
  skipAutosave.value = false;
}

onMounted(async () => {
  await initialize();
});

onStateChanged((payload) => {
  currentState.value = payload;
  syncSliderToState(payload);
});

onSettingsSaved((payload) => {
  lastSavedSnapshot.value = serializeSettings(payload);
  skipAutosave.value = true;
  settings.value = payload;
  syncAppearanceMode(payload.appearanceMode);
  skipAutosave.value = false;
});

onStartupStateChanged((payload) => {
  startupState.value = payload;
});

watch(
  () => (settings.value ? serializeAutosaveSettings(settings.value) : null),
  (nextSnapshot, previousSnapshot) => {
    if (!nextSnapshot || nextSnapshot === previousSnapshot) {
      return;
    }

    queueAutosave();
  }
);
</script>

<template>
  <main
    v-if="settings"
    class="flex h-screen flex-col overflow-hidden px-3 py-3 text-[var(--text)] sm:px-6 sm:py-6"
  >

    <p class="m-0 text-[0.9rem] uppercase tracking-[0.04em] text-[var(--muted)] absolute">Dimsome</p>

    <section class="mx-auto flex w-full max-w-5xl flex-none flex-col items-center gap-[18px] text-center">
      
      <div class="text-[clamp(2rem,4vw,3.5rem)] leading-none font-bold text-[var(--accent)]">
        {{ currentBrightnessPercent }}% brightness
      </div>
      <div class="glass-card mx-auto w-full max-w-[720px] rounded-full px-6 py-[18px] max-md:rounded-[28px] max-md:px-[18px] max-md:py-4">
        <AppSlider
          v-model="sliderBrightness"
          class="w-full"
          :min="5"
          :max="100"
          :step="0.1"
          @update:model-value="(value) => applyBrightnessWhileDragging(Array.isArray(value) ? value[0] : value)"
          @slideend="applyBrightnessFromSlider"
        />
      </div>
    </section>

    <section class="mx-auto mt-[22px] grid w-full max-w-5xl flex-none justify-items-center">
      <SelectButton
        v-model="selectedPanel"
        :options="panelOptions"
        option-label="label"
        option-value="value"
        :allow-empty="false"
        class="mx-auto !w-auto"
      />
    </section>

    <section
      v-if="selectedPanel === 'schedule'"
      class="mx-auto mt-[22px] grid min-h-0 w-full max-w-5xl flex-1 justify-items-center overflow-hidden"
    >
      <div :class="[cardClass, 'flex min-h-0 w-full max-w-[980px] flex-col overflow-hidden']">
        <div class="flex flex-wrap items-start justify-between gap-4">
          <label class="flex items-center gap-3 px-4 py-3 text-left">
            <ToggleSwitch v-model="settings.scheduleEnabled" />
            <span class="text-[0.9rem] font-semibold uppercase tracking-[0.04em] text-[var(--muted)]">Enable schedule</span>
          </label>

          <div v-if="!isFollowingSchedule" class="inline-flex items-center justify-center gap-2.5 text-base text-[var(--muted)] float-right" @click="resumeSchedule">
            <span>{{ !settings.scheduleEnabled ? "Schedule disabled" : ( isFollowingSchedule ? "Following schedule" : "Schedule paused (click to resume)" ) }}</span>
          </div>
        </div>

        <div class="mt-[18px] min-h-0 flex-1 overflow-auto pr-1">
          <div
            :class="[
              'grid gap-2 pb-1 transition-opacity',
              settings.scheduleEnabled ? 'opacity-100' : 'opacity-55'
            ]"
          >
          <div
            class="grid min-w-[760px] grid-cols-[minmax(0,1fr)_140px_140px_80px_auto] items-center gap-3 px-3 text-[0.82rem] font-semibold uppercase tracking-[0.04em] text-[var(--muted)]"
          >
            <span>Time</span>
            <span>Brightness %</span>
            <span>Fade duration</span>
            <span class="text-center">Enabled</span>
            <span class="text-right">Action</span>
          </div>
          <div
            v-for="point in settings.schedulePoints"
            :key="point.id"
            class="glass-card-strong grid min-w-[760px] grid-cols-[minmax(0,1fr)_140px_140px_80px_auto] items-center gap-3 rounded-[16px] px-3 py-2.5"
          >
            <div :class="fieldClass">
              <DatePicker
                :model-value="scheduleTimeToDate(point.timeOfDay)"
                time-only
                hour-format="24"
                show-icon
                icon="pi pi-clock"
                icon-display="input"
                manual-input
                :disabled="!settings.scheduleEnabled"
                fluid
                @update:model-value="updateScheduleTime(point, $event)"
              />
            </div>
            <div :class="fieldClass">
              <InputNumber
                :model-value="toBrightnessPercent(point.targetDimPercent)"
                @update:model-value="point.targetDimPercent = toDimPercent($event ?? 100)"
                incrementButtonClass="mt-1" decrementButtonClass="mb-1"
                showButtons
                :min="5"
                :max="100"
                :disabled="!settings.scheduleEnabled"
                fluid
              />
            </div>
            <div :class="fieldClass">
              <InputNumber v-model="point.transitionMinutes" showButtons :min="0" :max="1439" :disabled="!settings.scheduleEnabled" fluid incrementButtonClass="mt-1" decrementButtonClass="mb-1" />
            </div>
            <div class="flex justify-center">
              <ToggleSwitch v-model="point.enabled" :disabled="!settings.scheduleEnabled" />
            </div>
            <Button
              label="Remove"
              severity="secondary"
              variant="outlined"
              class="w-full min-w-[96px]"
              :disabled="!settings.scheduleEnabled"
              @click="removeSchedulePoint(point.id)"
            />
          </div>
          </div>

          <Button
            label="Add Schedule Point"
            severity="secondary"
            variant="outlined"
            class="mt-[18px] mb-9 sm:w-auto"
            :disabled="!settings.scheduleEnabled"
            @click="addSchedulePoint"
          />
        </div>
      </div>
    </section>

    <section
      v-else
      class="mx-auto mt-[22px] grid w-full max-w-[760px] flex-1 gap-[18px] overflow-y-auto pb-1"
    >
      <div :class="cardClass">
        <div :class="sectionLabelClass">Appearance</div>
        <label :class="[fieldClass, 'mt-4']">
          <span :class="fieldLabelClass">Color scheme</span>
          <Select
            v-model="selectedAppearanceMode"
            :options="appearanceModeOptions"
            option-label="label"
            option-value="value"
            fluid
          />
        </label>
        <p class="mt-3 text-[var(--muted)]">
          If you leave this on Follow system, PrimeVue tracks the operating system color scheme.
        </p>
      </div>

      <div :class="cardClass">
        <div :class="sectionLabelClass">Dimming</div>
        <label :class="[fieldClass, 'mt-4']">
          <span :class="fieldLabelClass">Method</span>
          <Select
            v-model="settings.dimmingMethod"
            :options="dimmingMethodOptions"
            option-label="label"
            option-value="value"
            option-disabled="disabled"
            fluid
          />
        </label>
        <p class="mt-3 text-[var(--muted)]">
          {{ dimmingMethodSummary }}
        </p>
      </div>

      <div :class="cardClass">
        <div :class="sectionLabelClass">Automation</div>
        <label :class="[fieldClass, 'mt-4 grid-cols-[1fr_auto] items-center']">
          <span :class="fieldLabelClass">Launch at sign-in</span>
          <ToggleSwitch
            v-model="settings.startupEnabled"
            :disabled="startupState ? !startupState.canChange : false"
          />
        </label>
        <p class="mt-3 text-[var(--muted)]">{{ startupState?.statusText ?? "Loading startup state..." }}</p>
        <label :class="[fieldClass, 'mt-4']">
          <span :class="fieldLabelClass">Brightness step size</span>
          <AppSlider v-model="settings.dimStepPercent" :min="1" :max="25" :step="1" />
        </label>
        <p class="mt-3 text-[var(--muted)]">{{ brightnessStepSummary }}</p>
      </div>

      <div :class="cardClass">
        <div :class="sectionLabelClass">Hotkeys</div>
        <label :class="[fieldClass, 'mt-4']">
          <span :class="fieldLabelClass">Decrease brightness key</span>
          <InputText v-model="settings.manualHotkeys.dimMore.key" fluid @blur="saveHotkeys" />
        </label>
        <label :class="[fieldClass, 'mt-4']">
          <span :class="fieldLabelClass">Increase brightness key</span>
          <InputText v-model="settings.manualHotkeys.dimLess.key" fluid @blur="saveHotkeys" />
        </label>
        <p class="mt-3 text-[var(--muted)]">
          Modifier handling is preserved in the backend JSON contract; this first pass exposes the key names directly.
        </p>
      </div>
    </section>
  </main>
</template>

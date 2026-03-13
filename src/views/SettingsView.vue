<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
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
const isApplyingSliderBrightness = ref(false);
const pendingSliderBrightness = ref<number | null>(null);
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
document.addEventListener('contextmenu', event => event.preventDefault());

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
      <div class="inline-flex items-center justify-center gap-2.5 text-base text-[var(--muted)]">
        <span>{{ !settings.scheduleEnabled ? "Schedule disabled" : ( isFollowingSchedule ? "Following schedule" : "Schedule paused" ) }}</span>
        <Button
          v-if="!isFollowingSchedule || !settings.scheduleEnabled"
          label="▶"
          text
          rounded
          aria-label="Resume schedule"
          class="!w-auto min-w-10"
          @click="resumeSchedule"
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
          <div>
            <div :class="sectionLabelClass">Schedule</div>
            <p class="mt-2 text-[var(--muted)]">
              Each point defines the target brightness level and how many minutes the ramp should take before it lands.
            </p>
          </div>
          <label class="flex items-center gap-3 rounded-[16px] border border-[var(--stroke)] px-4 py-3 text-left">
            <input v-model="settings.scheduleEnabled" type="checkbox" class="h-4 w-4 accent-[var(--accent)]" />
            <span class="text-[0.9rem] font-semibold uppercase tracking-[0.04em] text-[var(--muted)]">Enable schedule</span>
          </label>
        </div>

        <div class="mt-[18px] min-h-0 flex-1 overflow-y-auto pr-1">
          <div
            :class="[
              'grid gap-[18px] pb-1 transition-opacity',
              settings.scheduleEnabled ? 'opacity-100' : 'opacity-55'
            ]"
          >
          <div
            v-for="point in settings.schedulePoints"
            :key="point.id"
            class="glass-card-strong grid items-end gap-3 rounded-[18px] p-[14px] xl:grid-cols-[minmax(0,1fr)_140px_140px_80px_auto]"
          >
            
            <label :class="fieldClass">
              <span :class="fieldLabelClass">Time</span>
              <DatePicker
                :model-value="scheduleTimeToDate(point.timeOfDay)"
                time-only
                hour-format="24"
                show-icon
                icon="pi pi-clock"
                icon-display="input"
                :manual-input="false"
                :disabled="!settings.scheduleEnabled"
                fluid
                @update:model-value="updateScheduleTime(point, $event)"
              />
            </label>
            <label :class="fieldClass">
              <span :class="fieldLabelClass">Brightness %</span>
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
            </label>
            <label :class="fieldClass">
              <span :class="fieldLabelClass">Fade duration</span>
              <InputNumber v-model="point.transitionMinutes" showButtons :min="0" :max="1439" :disabled="!settings.scheduleEnabled" fluid incrementButtonClass="mt-1" decrementButtonClass="mb-1" />
            </label>
            <label :class="fieldClass">
              <span :class="fieldLabelClass" class="text-center mb-4">Enabled</span>
              <div class="text-center"><ToggleSwitch class="mb-3" v-model="point.enabled" :disabled="!settings.scheduleEnabled" /></div>
            </label>
            <Button
              label="Remove"
              severity="secondary"
              variant="outlined"
              class="max-xl:w-full"
              :disabled="!settings.scheduleEnabled"
              @click="removeSchedulePoint(point.id)"
            />
          </div>
          </div>
        </div>

        <Button
          label="Add Schedule Point"
          severity="secondary"
          variant="outlined"
          class="mt-[18px] sm:w-auto"
          :disabled="!settings.scheduleEnabled"
          @click="addSchedulePoint"
        />
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
          <InputText v-model="settings.manualHotkeys.dimMore.key" fluid />
        </label>
        <label :class="[fieldClass, 'mt-4']">
          <span :class="fieldLabelClass">Increase brightness key</span>
          <InputText v-model="settings.manualHotkeys.dimLess.key" fluid />
        </label>
        <p class="mt-3 text-[var(--muted)]">
          Modifier handling is preserved in the backend JSON contract; this first pass exposes the key names directly.
        </p>
      </div>

      <div :class="cardClass">
        <div :class="sectionLabelClass">Actions</div>
        <div class="mt-4 grid gap-3 md:grid-cols-3">
          <Button label="Save Settings" @click="save" />
          <Button label="Pause Schedule" severity="secondary" variant="outlined" @click="pauseSchedule" />
          <Button label="Resume Schedule" severity="secondary" variant="outlined" @click="resumeSchedule" />
        </div>
        <p class="mt-3 text-[var(--muted)]">{{ statusMessage }}</p>
      </div>
    </section>
  </main>
</template>





<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import Button from "primevue/button";
import DatePicker from "primevue/datepicker";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import ToggleSwitch from "primevue/toggleswitch";
import AppSlider from "../components/AppSlider.vue";
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

// Track the persisted settings plus the live dimming/runtime state shown in the UI.
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

// Keep select options local so the template can stay declarative.
const appearanceModeOptions: Array<{ label: string; value: "system" | AppearanceMode }> = [
  { label: "Follow system", value: "system" },
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" }
];
const cardClass = "glass-card rounded-[24px] p-5";
const settingsSectionClass = "grid gap-6";
const sectionLabelClass = "text-[0.9rem] uppercase tracking-[0.04em] text-[var(--muted)]";
const fieldClass = "grid gap-1.5";
const fieldLabelClass = "text-[0.9rem] uppercase tracking-[0.04em] text-[var(--muted)]";

// Disable unavailable dimming engines without hiding them from the user.
const dimmingMethodOptions = computed<Array<{ label: string; value: DimmingMethod; disabled?: boolean }>>(() => [
  { label: "Black overlay", value: "overlay" },
  { label: "Gamma / LUT", value: "gamma" },
  {
    label: "Magnification",
    value: "magnification",
    disabled: !(dimmingCapabilities.value?.magnificationAvailable ?? false)
  }
]);

// Summarize the tradeoffs so the selector stays short and the details stay readable.
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

    // Store "system" as an undefined override so the backend JSON stays compact.
    settings.value.appearanceMode = value === "system" ? undefined : value;
    syncAppearanceMode(settings.value.appearanceMode);
  }
});

// Disable the native right-click context menu so the desktop window feels more app-like.
document.addEventListener("contextmenu", (event) => event.preventDefault());

function toBrightnessPercent(dimPercent: number) {
  return 100 - dimPercent;
}

function toDimPercent(brightnessPercent: number) {
  return Math.min(99, Math.max(0, 100 - brightnessPercent));
}

// Reflect backend state changes back into the slider's brightness-oriented UI.
function syncSliderToState(state: EffectiveDimState | null) {
  sliderBrightness.value = 100 - (state?.currentDimPercent ?? 0);
}

// Fail loudly if a settings-only action runs before initialization finishes.
function ensureSettings() {
  if (!settings.value) {
    throw new Error("Settings are not loaded.");
  }

  return settings.value;
}

// Clone before save so in-flight backend updates do not mutate the active form model.
function cloneSettings(model: AppSettings): AppSettings {
  return JSON.parse(JSON.stringify(model)) as AppSettings;
}

// Use full serialization for exact equality checks after a save completes.
function serializeSettings(model: AppSettings) {
  return JSON.stringify(model);
}

// Ignore fields that are updated externally so we only autosave user-edited settings.
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

// Collapse multiple quick edits into a single save loop instead of racing requests.
function queueAutosave() {
  if (skipAutosave.value || !settings.value) {
    return;
  }

  saveQueued.value = true;
  void flushAutosaveQueue();
}

// Only touch startup registration when the requested toggle differs from the applied state.
function shouldUpdateStartupRegistration(startupEnabled: boolean) {
  if (!startupState.value) {
    return true;
  }

  return startupState.value.isEnabled !== startupEnabled;
}

// Serialize saves so startup registration changes finish before settings persistence.
async function flushAutosaveQueue() {
  if (isSaving.value || !settings.value) {
    return;
  }

  isSaving.value = true;

  try {
    while (saveQueued.value && settings.value) {
      saveQueued.value = false;
      const snapshot = cloneSettings(settings.value);

      // Skip registry writes when unrelated edits keep the same startup preference.
      if (shouldUpdateStartupRegistration(snapshot.startupEnabled)) {
        const startup = await setStartupEnabled(snapshot.startupEnabled);
        startupState.value = startup;
        snapshot.startupEnabled = startup.isEnabled;
      }

      // Persist the normalized settings and then replace the form with the saved copy.
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
    // Keep the UI responsive even if persistence fails; the console has the details.
    console.error("Failed to auto-save settings.", error);
  } finally {
    isSaving.value = false;
  }
}

// Seed new points one hour after the latest scheduled entry to reduce manual cleanup.
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

// Remove points immutably so Vue updates the list without edge cases.
function removeSchedulePoint(id: string) {
  const model = ensureSettings();
  model.schedulePoints = model.schedulePoints.filter((point) => point.id !== id);
}

// Convert a stored HH:mm:ss value into a DatePicker-friendly date object.
function scheduleTimeToDate(timeOfDay: string) {
  const [hour = "00", minute = "00", second = "00"] = timeOfDay.split(":");
  const value = new Date();
  value.setHours(Number.parseInt(hour, 10), Number.parseInt(minute, 10), Number.parseInt(second, 10), 0);
  return value;
}

// Only commit valid DatePicker values back into the serialized schedule string.
function updateScheduleTime(point: AppSettings["schedulePoints"][number], value: Date | Date[] | (Date | null)[] | undefined | null) {
  if (!(value instanceof Date)) {
    return;
  }

  const hour = value.getHours().toString().padStart(2, "0");
  const minute = value.getMinutes().toString().padStart(2, "0");
  point.timeOfDay = `${hour}:${minute}:00`;
}

// Hotkeys save on blur so partial edits do not re-register bindings mid-typing.
function saveHotkeys() {
  queueAutosave();
}

// PrimeVue's slideend event may contain range arrays, so normalize it before saving.
async function applyBrightnessFromSlider(event: { value: number | number[] }) {
  const nextBrightness = Array.isArray(event.value) ? event.value[0] : event.value;
  await applyBrightnessWhileDragging(nextBrightness);
}

// Coalesce rapid drag updates so only the latest brightness gets sent after each await.
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
      // Manual changes temporarily override the schedule until the next transition.
      currentState.value = await applyManualDim(toDimPercent(brightnessToApply));
      syncSliderToState(currentState.value);
    }
  } finally {
    isApplyingSliderBrightness.value = false;
  }
}

// Load everything the window needs up front so the first rendered view is complete.
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
  // Delay the initial fetch until the component is mounted inside the Tauri window.
  await initialize();
});

// Mirror backend state broadcasts into the live brightness display.
onStateChanged((payload) => {
  currentState.value = payload;
  syncSliderToState(payload);
});

// Accept backend-normalized settings so the form always reflects the persisted truth.
onSettingsSaved((payload) => {
  lastSavedSnapshot.value = serializeSettings(payload);
  skipAutosave.value = true;
  settings.value = payload;
  syncAppearanceMode(payload.appearanceMode);
  skipAutosave.value = false;
});

// Keep the sign-in toggle status text updated after registry changes.
onStartupStateChanged((payload) => {
  startupState.value = payload;
});

watch(
  () => (settings.value ? serializeAutosaveSettings(settings.value) : null),
  (nextSnapshot, previousSnapshot) => {
    if (!nextSnapshot || nextSnapshot === previousSnapshot) {
      return;
    }

    // Autosave whenever the meaningful settings snapshot changes after initialization.
    queueAutosave();
  }
);
</script>

<template>
  <main
    v-if="settings"
    class="flex h-screen flex-col overflow-hidden px-3 py-3 text-[var(--text)] sm:px-6 sm:py-6"
  >
    <!-- App name on the left, cogwheel settings toggle on the right. -->
    <div class="flex items-center justify-between h-6">
      <!--<p class="m-0 text-[0.9rem] uppercase tracking-[0.04em] text-[var(--muted)]">Dimsome</p>-->

      <!-- Top-right cogwheel / back buttons-->
      <div class="">
          <button
              v-if="selectedPanel === 'schedule'"
                  class="flex items-center justify-center rounded-full p-1.5 text-[var(--muted)] transition-colors hover:text-[var(--text)] cursor-pointer"
              title="Settings"
              @click="selectedPanel = 'settings'"
            >
                  <i class="pi pi-cog" style="font-size: 1.3rem;" />
            </button>

             <!-- Navigate back to the schedule panel. -->
            <div 
              v-if="selectedPanel === 'settings'"
            class=""
            >
              
              <a
                class="float-right inline-flex items-center justify-center gap-2.5 text-base text-[var(--muted)] cursor-pointer transition-colors hover:text-[var(--text)]"
                @click="selectedPanel = 'schedule'"
              >
                <i class="pi pi-arrow-left text-[0.8rem]" />
                <span>Back to Schedule</span>
              </a>
            </div>
      </div>
      

     
    </div>

    <section class="mx-auto flex w-full max-w-5xl flex-none flex-col items-center gap-5 text-center mb-4">
      <!-- Surface the current brightness first because it is the primary action. -->
      <div class="text-[2.4rem] font-semibold leading-none text-[var(--accent)]">
        {{ currentBrightnessPercent }}% brightness
      </div>
      <div class="glass-card mx-auto w-full max-w-[720px] rounded-full px-6 py-5 max-md:rounded-[28px] max-md:px-[18px] max-md:py-4">
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

    <section
      class=""
    >
      <div class="h-14 w-full flex justify-center">
      <!-- Clicking the paused badge is the quickest way back to automatic mode. -->
      <div
        v-if="!isFollowingSchedule"
        class="float-right inline-flex items-center justify-center gap-2.5 text-base font-semibold text-[var(--muted)] cursor-pointer transition-colors hover:text-[var(--text)] mt-2 mb-6"
        @click="resumeSchedule"
      >
        <span>{{ !settings.scheduleEnabled ? "Schedule disabled" : (isFollowingSchedule ? "Following schedule" : "Schedule Override - Click to Resume") }}</span>
      </div>

      <!-- The master toggle dims the whole schedule editor without deleting points. -->
      <label 
        v-else
        class="inline-flex w-fit items-center gap-3 px-4 py-3 text-left justify-self-center cursor-pointer mb-4 ">
        <ToggleSwitch v-model="settings.scheduleEnabled" />
        <span class="text-[0.9rem] font-semibold uppercase tracking-[0.04em] text-[var(--muted)]">Enable schedule</span>
      </label>
      </div>
    </section> 


    <section
      v-if="selectedPanel === 'schedule'"
      class="mx-auto grid min-h-0 w-full max-w-5xl flex-1 overflow-hidden"
    > <!-- justify-items-center -->


      
      <div :class="[cardClass, 'flex min-h-0 w-full max-w-[980px] flex-col overflow-auto']">
        <div class="flex flex-wrap items-start justify-between gap-4 ">
          

          
        </div>

        <div class="mt-[18px] min-h-0 flex-1  pr-1">
          <div
            :class="[
              'grid gap-2 pb-1 transition-opacity',
              settings.scheduleEnabled ? 'opacity-100' : 'opacity-55'
            ]"
          >
            <!-- Keep the schedule list table-like so time, brightness, and duration line up. -->
            <div
              class="grid min-w-[760px] grid-cols-[minmax(0,1fr)_140px_140px_80px_auto] items-center gap-3 px-3 text-[0.82rem] font-semibold uppercase tracking-[0.04em] text-[var(--muted)]"
            >
              <span>Time</span>
              <span>Brightness %</span>
              <span>Fade duration</span>
              <span class="text-center">Enabled</span>
              <span class="text-right">Action</span>
            </div>

            <!-- Each row stays fully editable so schedule changes can be made in-place. -->
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
                  incrementButtonClass="mt-1"
                  decrementButtonClass="mb-1"
                  showButtons
                  :min="5"
                  :max="100"
                  :disabled="!settings.scheduleEnabled"
                  fluid
                />
              </div>
              <div :class="fieldClass">
                <InputNumber
                  v-model="point.transitionMinutes"
                  showButtons
                  :min="0"
                  :max="1439"
                  :disabled="!settings.scheduleEnabled"
                  fluid
                  incrementButtonClass="mt-1"
                  decrementButtonClass="mb-1"
                />
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

          <!-- Add points beneath the list so the table stays the main focus. -->
          <Button
            label="Add Schedule Point"
            severity="secondary"
            variant="outlined"
            class="mb-9 mt-[18px] sm:w-auto"
            :disabled="!settings.scheduleEnabled"
            @click="addSchedulePoint"
          />
        </div>
      </div>
    </section>

    <section
      v-else
      class="mx-auto grid min-h-0 w-full max-w-5xl flex-1 overflow-hidden"
    >

      

      <div :class="[cardClass, 'min-h-0 overflow-y-auto p-6']">
        <div :class="settingsSectionClass">
          <section>
            <!-- Appearance stays separate so theme changes do not get lost among dimming options. -->
            <div :class="sectionLabelClass" class="text-center">Settings</div>
            <label :class="[fieldClass, 'mt-6']">
              <span :class="fieldLabelClass">Theme</span>
              <Select
                v-model="selectedAppearanceMode"
                :options="appearanceModeOptions"
                option-label="label"
                option-value="value"
                fluid
              />
            </label>
            <!--<p class="mt-3 text-[var(--muted)]">
              If you leave this on Follow system, PrimeVue tracks the operating system color scheme.
            </p>-->
          </section>

          <section>
            <!-- Explain the available dimming engines without overwhelming the main schedule UI. -->
            <label :class="[fieldClass, 'mt-2']">
              <span :class="fieldLabelClass">Dimming Method</span>
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
          </section>

          <section>
            <!-- Group automation-related toggles so startup and manual step size live together. -->
            <label :class="[fieldClass, 'mt-3 inline-flex items-center']">
              <span :class="fieldLabelClass" class="pr-3">Launch Dimsome at sign-in</span>
              <ToggleSwitch
                v-model="settings.startupEnabled"
                :disabled="startupState ? !startupState.canChange : false"
              />
            </label>
            <!--<p class="mt-3 text-[var(--muted)]">{{ startupState?.statusText ?? "Loading startup state..." }}</p>-->
            
          </section>

          <section>
            <!-- Keep hotkey editing intentionally lightweight until a richer picker exists. -->
            <div :class="sectionLabelClass" class="text-center">Hotkeys</div>
            <label :class="[fieldClass, 'mt-6']">
              <span :class="fieldLabelClass">Decrease brightness key</span>
              <InputText v-model="settings.manualHotkeys.dimMore.key" fluid @blur="saveHotkeys" />
            </label>
            <label :class="[fieldClass, 'mt-6']">
              <span :class="fieldLabelClass">Increase brightness key</span>
              <InputText v-model="settings.manualHotkeys.dimLess.key" fluid @blur="saveHotkeys" />
            </label>
            <label :class="[fieldClass, 'mt-6']">
              <span :class="fieldLabelClass">Brightness step size</span>
              <AppSlider v-model="settings.dimStepPercent" :min="1" :max="25" :step="1" />
            </label>
            <p class="mt-3 text-[var(--muted)]">{{ brightnessStepSummary }}</p>
          </section>
        </div>
      </div>
    </section>
  </main>
</template>
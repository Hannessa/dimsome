<script setup lang="ts">
import { onMounted, ref } from "vue";
import { applyManualDim, exitApp, getEffectiveState, pauseSchedule, resumeSchedule, showSettingsWindow } from "../lib/api";
import { onStateChanged } from "../lib/events";
import type { EffectiveDimState } from "../types/app";

const currentState = ref<EffectiveDimState | null>(null);
const requestedDimPercent = ref(0);

function modeText(mode: EffectiveDimState["mode"] | undefined) {
  if (mode === "Manual") return "Manual override";
  if (mode === "Paused") return "Schedule paused";
  return "Following schedule";
}

async function initialize() {
  currentState.value = await getEffectiveState();
  requestedDimPercent.value = currentState.value.currentDimPercent;
}

async function updateDim() {
  currentState.value = await applyManualDim(requestedDimPercent.value);
}

onMounted(async () => {
  await initialize();
});

onStateChanged((payload) => {
  currentState.value = payload;
  requestedDimPercent.value = payload.currentDimPercent;
});
</script>

<template>
  <main class="page page-quick">
    <section class="quick-card">
      <div>
        <p class="eyebrow">Dimsome</p>
        <div class="muted">{{ modeText(currentState?.mode) }}</div>
        <div class="state-value">{{ Math.round(currentState?.currentDimPercent ?? 0) }}% dim</div>
      </div>

      <label class="field">
        <span>Manual dim level</span>
        <input type="range" min="0" max="95" step="1" v-model.number="requestedDimPercent" @input="updateDim" />
      </label>

      <div class="action-row">
        <button class="button secondary" @click="pauseSchedule">Pause</button>
        <button class="button secondary" @click="resumeSchedule">Resume</button>
      </div>

      <button class="button" @click="showSettingsWindow">Open Settings</button>
      <button class="button danger" @click="exitApp">Exit Dimsome</button>
    </section>
  </main>
</template>

<script setup lang="ts">
import SettingsView from "./views/SettingsView.vue";
import QuickPanelView from "./views/QuickPanelView.vue";

const params = new URLSearchParams(window.location.search);
const kind = params.get("window") ?? "settings";
</script>

<template>
  <QuickPanelView v-if="kind === 'quick-panel'" />
  <SettingsView v-else />
</template>

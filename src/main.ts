import { createApp } from "vue";
import { definePreset } from "@primeuix/themes";
import Material from "@primeuix/themes/material";
import PrimeVue from "primevue/config";
import "primeicons/primeicons.css";
import App from "./App.vue";
import { getSettings } from "./lib/api";
import { syncAppearanceMode } from "./lib/theme";
import "./styles.css";

// Start from PrimeVue's Material tokens and tint them to Dimsome's palette.
const DimsomePreset = definePreset(Material, {
  semantic: {
    primary: {
      50: "{pink.50}",
      100: "{pink.100}",
      200: "{pink.200}",
      300: "{pink.300}",
      400: "{pink.400}",
      500: "{pink.500}",
      600: "{pink.600}",
      700: "{pink.700}",
      800: "{pink.800}",
      900: "{pink.900}",
      950: "{pink.950}"
    }
  },
  components: {
    slider: {
      track: {
        background: "transparent"
      }
    }
  }
});

async function bootstrap() {
  // Create the Vue app before we fetch persisted settings and theme preferences.
  const app = createApp(App);
  // Apply the saved appearance mode before mount so the first paint is correct.
  const settings = await getSettings().catch(() => null);
  syncAppearanceMode(settings?.appearanceMode);

  // Install PrimeVue once with the custom preset and app-level dark selector.
  app.use(PrimeVue, {
    ripple: true,
    theme: {
      preset: DimsomePreset,
      options: {
        darkModeSelector: ".app-dark"
      }
    }
  });

  // Mount after theme classes and PrimeVue configuration are in place.
  app.mount("#app");
}

void bootstrap();
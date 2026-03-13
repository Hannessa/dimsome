import { createApp } from "vue";
import { definePreset } from "@primeuix/themes";
import Material from "@primeuix/themes/material";
import PrimeVue from "primevue/config";
import App from "./App.vue";
import { getSettings } from "./lib/api";
import { syncAppearanceMode } from "./lib/theme";
import "./styles.css";

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
  const app = createApp(App);
  const settings = await getSettings().catch(() => null);
  syncAppearanceMode(settings?.appearanceMode);

  app.use(PrimeVue, {
    ripple: true,
    theme: {
      preset: DimsomePreset,
      options: {
        darkModeSelector: ".app-dark"
      }
    }
  });

  app.mount("#app");
}

void bootstrap();

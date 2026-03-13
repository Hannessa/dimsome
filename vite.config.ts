import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  // Keep the frontend pipeline minimal: Vue SFC support plus Tailwind v4.
  plugins: [vue(), tailwindcss()],
  clearScreen: false,
  server: {
    // Tauri expects a predictable local dev host and port. Not used in release version.
    host: "127.0.0.1",
    port: 1420,
    strictPort: true
  }
});
export type AppearanceMode = "light" | "dark";

const MEDIA_QUERY = "(prefers-color-scheme: dark)";
const APP_DARK_CLASS = "app-dark";
const APP_LIGHT_CLASS = "app-light";

let activePreference: AppearanceMode | null = null;
let mediaQueryList: MediaQueryList | null = null;

function getMediaQueryList() {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return null;
  }

  if (!mediaQueryList) {
    mediaQueryList = window.matchMedia(MEDIA_QUERY);
    mediaQueryList.addEventListener("change", applyThemeClasses);
  }

  return mediaQueryList;
}

function prefersDarkMode() {
  return getMediaQueryList()?.matches ?? false;
}

function applyThemeClasses() {
  if (typeof document === "undefined") {
    return;
  }

  const useDarkMode = activePreference === "dark" || (activePreference === null && prefersDarkMode());
  document.documentElement.classList.toggle(APP_DARK_CLASS, useDarkMode);
  document.documentElement.classList.toggle(APP_LIGHT_CLASS, !useDarkMode);
}

export function syncAppearanceMode(preference?: AppearanceMode | null) {
  activePreference = preference ?? null;
  applyThemeClasses();
}

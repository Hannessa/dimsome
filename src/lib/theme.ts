export type AppearanceMode = "light" | "dark";

const MEDIA_QUERY = "(prefers-color-scheme: dark)";
const APP_DARK_CLASS = "app-dark";
const APP_LIGHT_CLASS = "app-light";

let activePreference: AppearanceMode | null = null;
let mediaQueryList: MediaQueryList | null = null;

// Cache the media query object so we only register one system-theme listener.
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

// Fall back to light mode when the browser cannot resolve system appearance.
function prefersDarkMode() {
  return getMediaQueryList()?.matches ?? false;
}

// Toggle explicit root classes so both CSS and PrimeVue can react to one source of truth.
function applyThemeClasses() {
  if (typeof document === "undefined") {
    return;
  }

  const useDarkMode = activePreference === "dark" || (activePreference === null && prefersDarkMode());
  document.documentElement.classList.toggle(APP_DARK_CLASS, useDarkMode);
  document.documentElement.classList.toggle(APP_LIGHT_CLASS, !useDarkMode);
}

export function syncAppearanceMode(preference?: AppearanceMode | null) {
  // Null means "follow system", while an explicit value overrides the media query.
  activePreference = preference ?? null;
  applyThemeClasses();
}
// App-wide state: the user's preferences, what we know about this run, and a
// tiny publish/subscribe so pages can react to each other.

import { api } from "./api.js";

const SETTINGS_KEY = "quadrant.settings";

const DEFAULTS = {
  theme: "auto", // "auto" | "dark" | "light"
  text: "standard", // "standard" | "large" | "larger"
  tempMinAgeHours: 24, // 0 switches the rule off
  recycle: true, // Large Files and Duplicates go to the Recycle Bin
  aiEndpoint: "http://127.0.0.1:11434", // local model server; Ollama by default
  aiModel: "", // chosen in Settings from the models the server offers
};

function loadSettings() {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}") };
  } catch {
    return { ...DEFAULTS };
  }
}

export const settings = loadSettings();

/** What this run knows. Pages read these and listen for changes. */
export const app = {
  version: "",
  isElevated: false,
  /** The user's profile folder, the default place to scan. */
  homeDir: "",
  /** The last Health Check report, or null. */
  health: null,
};

// ── Events ──────────────────────────────────────────────────────────────────

const listeners = new Map();

/** Events: "settings", "health", "busy", "cleaned", "elevation". */
export function on(event, handler) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event).add(handler);
  return () => listeners.get(event).delete(handler);
}

export function emit(event, detail) {
  const handlers = listeners.get(event);
  if (handlers) for (const handler of [...handlers]) handler(detail);
}

// ── Settings ────────────────────────────────────────────────────────────────

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

/** "dark" or "light", after resolving "auto" against Windows. */
export function resolvedTheme() {
  if (settings.theme === "auto") return darkQuery.matches ? "dark" : "light";
  return settings.theme;
}

export function applyAppearance() {
  const theme = resolvedTheme();
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.text = settings.text;
  // Tint the native title bar to match. Harmless if it fails.
  api.setTheme(theme).catch(() => {});
}

darkQuery.addEventListener("change", () => {
  if (settings.theme === "auto") applyAppearance();
});

export function setSetting(key, value) {
  settings[key] = value;
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // Storage can be unavailable; the setting still applies for this run.
  }
  if (key === "theme" || key === "text") applyAppearance();
  emit("settings", key);
}

/** The options every scan and clean is given. See CleanOptions in model.rs. */
export function cleanOptions() {
  return { tempMinAgeHours: settings.tempMinAgeHours };
}

// ── Busy pages ──────────────────────────────────────────────────────────────

const busyPages = new Set();

/** Mark a page as having a job running, so the sidebar can show it. */
export function setBusy(pageId, busy) {
  if (busy) busyPages.add(pageId);
  else busyPages.delete(pageId);
  emit("busy", { pageId, busy });
}

export const isBusy = (pageId) => busyPages.has(pageId);
